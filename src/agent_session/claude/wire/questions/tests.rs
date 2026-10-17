use super::*;
use crate::agent_session::claude::wire::text_of;
use serde_json::json;

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
