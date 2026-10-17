use super::*;

#[test]
fn a_request_is_one_line() {
    let line = encode(
        "panorama:1",
        "tab.move",
        &json!({"tab_id": "w1:t3", "insert_index": 0}),
    );
    assert_eq!(line.last(), Some(&b'\n'));
    assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1);
    let v: Value = serde_json::from_slice(&line).unwrap();
    assert_eq!(v["method"], "tab.move");
    assert_eq!(v["id"], "panorama:1");
}

/// panorama tests/test_tui.py::test_a_reply_that_is_missing_or_garbled_after_sending_is_no_reply
#[test]
fn a_missing_or_garbled_reply_is_never_success() {
    assert!(matches!(decode(b""), Err(HerdrError::NoReply(_))));
    assert!(matches!(decode(b"  \n"), Err(HerdrError::NoReply(_))));
    assert!(matches!(decode(b"not json\n"), Err(HerdrError::Garbled(_))));
    assert!(matches!(decode(b"[1]\n"), Err(HerdrError::Garbled(_))));
    assert!(decode(b"not json\n").unwrap_err().maybe_done());
}

#[test]
fn errors_are_refusals_and_empty_results_are_empty_objects() {
    let err =
        decode(br#"{"id":"x","error":{"code":"tab_not_found","message":"tab w9 not found"}}"#)
            .unwrap_err();
    assert_eq!(
        err,
        HerdrError::Refused {
            code: "tab_not_found".into(),
            message: "tab w9 not found".into()
        }
    );
    assert_eq!(decode(br#"{"id":"x","result":null}"#).unwrap(), json!({}));
    assert_eq!(decode(br#"{"id":"x"}"#).unwrap(), json!({}));
    assert_eq!(
        decode(b"{\"result\":{\"type\":\"ok\"}}\n{\"more\":1}\n").unwrap(),
        json!({"type": "ok"})
    );
}
