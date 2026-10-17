use super::*;
use serde_json::json;

#[test]
fn text_of_joins_the_text_blocks() {
    let content = json!([{"type": "text", "text": "a"}, {"type": "image"}, "x", {"text": 5}]);
    assert_eq!(text_of(Some(&content)), "a  5");
    assert_eq!(text_of(Some(&json!("plain"))), "plain");
    assert_eq!(text_of(None), "");
}
