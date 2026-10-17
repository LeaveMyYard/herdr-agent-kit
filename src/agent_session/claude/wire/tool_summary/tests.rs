use super::*;
use serde_json::json;

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
