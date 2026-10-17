use super::*;
use crate::agent_session::SessionId;
use crate::agent_session::claude::wire::Content;

#[test]
fn bad_and_odd_lines_never_parse() {
    assert_eq!(Line::parse(b"not json"), None);
    assert_eq!(Line::parse(br#"{"type":"user","message":"#), None);
    assert_eq!(Line::parse(b"[1,2]"), None);
    assert_eq!(Line::parse(b"\xff\xfe"), None);
    assert!(Line::parse(b"{\"a\":1}\r").is_some());
    assert!(Line::parse(b"\xef\xbb\xbf{\"a\":1}").is_some());
}

#[test]
fn a_lone_surrogate_escape_parses_like_python() {
    // expected values: Python 3.13 json.loads on the same lines, with each
    // lone surrogate read as U+FFFD (Python keeps the surrogate itself)
    let cases: [(&[u8], Option<&str>); 8] = [
        (br#"{"c":"\ud83d cut"}"#, Some("\u{fffd} cut")),
        (br#"{"c":"a\ude00"}"#, Some("a\u{fffd}")),
        (br#"{"c":"\ud83d\u0041"}"#, Some("\u{fffd}A")),
        (br#"{"c":"\ud83d\ude00"}"#, Some("\u{1f600}")),
        (br#"{"c":"\\ud83d"}"#, Some("\\ud83d")),
        (br#"{"c":"\uD83D"}"#, Some("\u{fffd}")),
        (br#"{"c":"\ud83d""#, None),
        (br#"{"c":"\ud83d\ud83d\ude00"}"#, Some("\u{fffd}\u{1f600}")),
    ];
    for (raw, want) in cases {
        let got = Line::parse(raw).map(|l| l.get("c").and_then(Value::as_str).map(str::to_string));
        assert_eq!(
            got,
            want.map(|w| Some(w.to_string())),
            "{}",
            String::from_utf8_lossy(raw)
        );
    }
}

#[test]
fn records_are_lenient() {
    let l = Line::parse(br#"{"type":"continued-in","sessionId":"ABCDEF01-2345-4abc-8def-0123456789ab","continuedInSessionId":"../x","isSidechain":"true"}"#).unwrap();
    assert_eq!(
        l.record(),
        Record::ContinuedIn {
            from: SessionId::parse("abcdef01-2345-4abc-8def-0123456789ab"),
            to: None,
            sidechain: false
        }
    );
    let l = Line::parse(br#"{"type":"custom-title","customTitle":5}"#).unwrap();
    assert_eq!(l.record(), Record::Other);
    let l = Line::parse(br#"{"type":"made-up","x":1}"#).unwrap();
    assert_eq!(l.record(), Record::Other);
    let l = Line::parse(br#"{"type":"mode","mode":"normal"}"#).unwrap();
    assert_eq!(l.record(), Record::Bookkeeping("mode"));
    let l = Line::parse(br#"{"type":"queue-operation","operation":"dequeue"}"#).unwrap();
    assert_eq!(l.record(), Record::Other);
    assert_eq!(l.notice_text(), None);
    let l = Line::parse(br#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"x"},7]},"toolUseResult":"Error: string form"}"#).unwrap();
    match l.record() {
        Record::User {
            content: Content::Blocks(b),
            tool_use_result: Some(Value::String(_)),
        } => {
            assert_eq!(b.len(), 1)
        }
        other => panic!("{other:?}"),
    }
}
