use super::*;

#[test]
fn only_sent_and_unanswered_requests_may_have_happened() {
    assert!(HerdrError::NoReply("timed out".into()).maybe_done());
    assert!(HerdrError::Garbled("x".into()).maybe_done());
    assert!(!HerdrError::NotRunning.maybe_done());
    assert!(!HerdrError::Unreachable("x".into()).maybe_done());
    let refused = HerdrError::Refused {
        code: "ui_busy".into(),
        message: "a popup pane is already open".into(),
    };
    assert!(!refused.maybe_done());
    assert!(refused.is_code("ui_busy"));
    assert_eq!(refused.to_string(), "ui_busy: a popup pane is already open");
    let plain = HerdrError::Refused {
        code: String::new(),
        message: "error: unknown machine 'nosuch'".into(),
    };
    assert_eq!(plain.code(), None);
    assert_eq!(plain.to_string(), "error: unknown machine 'nosuch'");
}
