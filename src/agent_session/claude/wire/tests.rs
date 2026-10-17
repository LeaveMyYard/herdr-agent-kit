use super::pytext::py_float_repr;
use super::*;
use crate::agent_session::{SessionId, Ts};
use serde_json::Value;
use serde_json::json;

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
fn wanted_lines_matches_wanted() {
    let lines: [&[u8]; 5] = [
        br#"{"type":"assistant","x":"\"name\":\"Bash\""}"#,
        br#"{"name":"Agent"}"#,
        br#"{"role":"user","content":"hi"}"#,
        br#"{"role":"user","content":[]}"#,
        b"<task-notification>",
    ];
    let by_one: Vec<usize> = (0..lines.len()).filter(|&i| wanted(lines[i])).collect();
    assert_eq!(wanted_lines(&lines), by_one);
    assert_eq!(by_one, vec![1, 2, 4]);
}

#[test]
fn timestamps_parse_like_python() {
    let cases = [
        (json!("2026-10-14T09:00:00.000Z"), Some(1_791_968_400.0)),
        (
            json!("2026-10-14T09:00:00.123+00:00"),
            Some(1_791_968_400.123),
        ),
        (
            json!("2026-10-14T09:00:00.1234567Z"),
            Some(1_791_968_400.123_456),
        ),
        (json!("2026-10-14T09:00:00,5Z"), Some(1_791_968_400.5)),
        (json!("2026-10-14T09:00+0130"), Some(1_791_963_000.0)),
        (json!("2026-10-14T09:00+01"), Some(1_791_964_800.0)),
        (json!("2026-10-14T09:00:00+00:00:30"), Some(1_791_968_370.0)),
        (json!("2026-10-14T09:00:00-00:00"), Some(1_791_968_400.0)),
        (json!("0001-01-01T00:00:00Z"), Some(-62_135_596_800.0)),
        (json!("2026-10-14T24:00:00Z"), None),
        (json!("2026-10-14T09:00:60Z"), None),
        (json!("2026-02-30T09:00:00Z"), None),
        (json!(" 2026-10-14T09:00:00Z"), None),
        (json!("+2026-10-14"), None),
        (json!(1_791_968_400_000u64), Some(1_791_968_400.0)),
        (json!(1_791_968_400), Some(1_791_968_400.0)),
        (json!(1.5), Some(1.5)),
        (json!("1791968400000"), Some(1_791_968_400.0)),
        (json!(" 1_791_968_400 "), Some(1_791_968_400.0)),
        (json!("1__0"), None),
        (json!("1.5"), None),
        (json!(true), None),
        (json!(null), None),
        (json!([1]), None),
        (json!({"a": 1}), None),
    ];
    for (v, want) in cases {
        assert_eq!(ts(&v).map(Ts::secs), want, "{v}");
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

#[test]
fn summaries_and_short_paths() {
    let m = |v: Value| v.as_object().cloned().unwrap();
    let bash = m(json!({"command": "ls", "description": "List files"}));
    assert_eq!(
        summarize_tool("Bash", Some(&bash), None),
        "Bash(List files)"
    );
    let read = m(json!({"file_path": "/r/proj/src/a.py"}));
    assert_eq!(
        summarize_tool("Read", Some(&read), Some("/r/proj")),
        "Read(src/a.py)"
    );
    assert_eq!(
        short_path("/r/proj-web/a.py", Some("/r/proj")),
        "/r/proj-web/a.py"
    );
    assert_eq!(
        summarize_tool("mcp__srv__tool", Some(&m(json!({"query": "q"}))), None),
        "tool(q)"
    );
    assert_eq!(summarize_tool("Weird", None, None), "Weird");
    let multi = m(json!({"command": "\n  \nfirst line\nsecond"}));
    assert_eq!(
        summarize_tool("Bash", Some(&multi), None),
        "Bash(first line)"
    );
    let long = m(json!({"description": "x".repeat(300)}));
    assert_eq!(
        summarize_tool("Agent", Some(&long), None),
        format!("Agent({})", "x".repeat(80))
    );
    assert_eq!(truncate_summary(&"y".repeat(201)).chars().count(), 200);
    assert!(truncate_summary(&"y".repeat(201)).ends_with('…'));
    assert_eq!(truncate_summary(" a\x1c b\u{3000}c "), "a b c");
    assert_eq!(
        short_path(&format!("/{}", "z".repeat(250)), None)
            .chars()
            .count(),
        200
    );
}

#[test]
fn splitlines_matches_python() {
    assert_eq!(
        py_splitlines("a\r\nb\rc\nd\x0be\u{2028}f"),
        vec!["a", "b", "c", "d", "e", "f"]
    );
    assert_eq!(py_splitlines("a\n"), vec!["a"]);
    assert_eq!(py_splitlines(""), Vec::<&str>::new());
    assert_eq!(py_splitlines("\n"), vec![""]);
}

/// Expected values from panorama's `wire.parse_notices` / `question_ids`
/// / `text_of` on main (`83590dc`), run over the same inputs.
#[test]
fn notices_parse_as_panorama_reads_them() {
    let n =
        |task: &str, tool_use: Option<&str>, status: Option<Ending>, uses: Option<u64>| Notice {
            task: task.into(),
            tool_use: tool_use.map(str::to_string),
            status,
            tool_uses: uses,
        };
    let cases: [(&str, Vec<Notice>); 7] = [
        (
            "<task-notification>\n<task-id>a1</task-id>\n<tool-use-id>toolu_1</tool-use-id>\n<status>completed</status>\n<usage><tool_uses>12</tool_uses></usage>\n</task-notification>",
            vec![n("a1", Some("toolu_1"), Some(Ending::Done), Some(12))],
        ),
        (
            // the notice a resumed session gets for its orphaned tasks: every id, not the marker
            "  <task-notification><task-id>b1</task-id>\n<task-id>b2</task-id>\n<task-id>__orphan_summary__:shell</task-id>\n<status>stopped</status></task-notification>",
            vec![
                n("b1", None, Some(Ending::Cancelled), None),
                n("b2", None, Some(Ending::Cancelled), None),
            ],
        ),
        (
            "<task-notification><task-id>m1</task-id><event>[Monitor expired after 30m]</event></task-notification>",
            vec![n("m1", None, Some(Ending::Done), None)],
        ),
        (
            "<task-notification><task-id>m1</task-id><event>line 4</event></task-notification>",
            vec![n("m1", None, None, None)],
        ),
        (
            // a repeated tag counts with its last value; two blocks in one carrier
            "<task-notification><task-id> x1 </task-id><status>failed</status><status>killed</status><tool_uses>n/a</tool_uses></task-notification><task-notification><task-id>x2</task-id><status>error</status></task-notification>",
            vec![
                n("x1", None, Some(Ending::Cancelled), None),
                n("x2", None, Some(Ending::Failed), None),
            ],
        ),
        (
            "text <task-notification><task-id>a1</task-id><status>completed</status></task-notification>",
            vec![],
        ),
        (
            "<task-notification><task-id>__orphan_summary__</task-id><status>stopped</status></task-notification>",
            vec![],
        ),
    ];
    for (text, want) in cases {
        assert_eq!(parse_notices(text), want, "{text}");
    }
}

#[test]
fn question_ids_and_result_text() {
    let ids = |t: &str| question_ids(t).into_iter().collect::<Vec<_>>();
    assert_eq!(ids("Recorded as Q12. Continue."), vec![12]);
    assert_eq!(ids("Q3 and Q45, notQ7, Q8x"), vec![3, 45]);
    assert_eq!(
        ids(r#"{"id": 9, "status": "already_open"} {"id":10,"status":"answered"}"#),
        vec![9]
    );
    let content = json!([{"type": "text", "text": "a"}, {"type": "image"}, "x", {"text": 5}]);
    assert_eq!(text_of(Some(&content)), "a  5");
    assert_eq!(text_of(Some(&json!("plain"))), "plain");
    assert_eq!(text_of(None), "");
}

#[test]
fn python_str_of_values() {
    assert_eq!(py_str(&json!("s")), "s");
    assert_eq!(py_str(&json!(5)), "5");
    assert_eq!(py_str(&json!(1.5)), "1.5");
    assert_eq!(py_str(&json!(true)), "True");
    assert_eq!(py_str(&Value::Null), "None");
    for (f, want) in [
        (1791969600.0, "1791969600.0"),
        (0.1, "0.1"),
        (1e16, "1e+16"),
        (0.00001, "1e-05"),
        (-2.5e-7, "-2.5e-07"),
        (123456789012345.6, "123456789012345.6"),
    ] {
        assert_eq!(py_float_repr(f), want);
    }
}
