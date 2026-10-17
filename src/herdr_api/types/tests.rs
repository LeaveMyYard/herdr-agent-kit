use super::*;
use serde_json::json;

#[test]
fn unknown_statuses_and_mistyped_fields_fall_back() {
    let a: AgentInfo = serde_json::from_value(json!({
        "pane_id": "w1:p2", "agent_status": "napping", "focused": null,
        "agent_session": {"value": "abc"}, "revision": 3
    }))
    .unwrap();
    assert_eq!(a.agent_status, AgentStatus::Unknown);
    assert!(!a.focused);
    assert_eq!(a.session_value(), Some("abc"));
    assert_eq!(a.extra["revision"], 3);
}

#[test]
fn a_session_of_another_kind_is_not_an_id() {
    let s: AgentSession = serde_json::from_value(json!({"kind": "name", "value": "x"})).unwrap();
    assert_eq!(s.id(), None);
    assert_eq!(s.value(), Some("x"));
}

#[test]
fn bad_list_items_are_skipped_and_argv_keeps_strings_and_ints() {
    let s: Snapshot =
        serde_json::from_value(json!({"agents": [1, {"agent": "claude"}], "panes": "x"})).unwrap();
    assert_eq!(s.agents.len(), 1);
    assert!(s.panes.is_empty());
    let p: ProcessInfo = serde_json::from_value(json!({"foreground_processes": [
        {"pid": 7, "argv": ["claude", 3, null, 1.5]}]}))
    .unwrap();
    assert_eq!(p.foreground_processes.unwrap()[0].argv, ["claude", "3"]);
    let p: ProcessInfo = serde_json::from_value(json!({})).unwrap();
    assert_eq!(p.foreground_processes, None);
}

#[test]
fn a_swap_is_done_unless_herdr_says_otherwise() {
    assert!(SwapResult::default().swapped());
    let s: SwapResult =
        serde_json::from_value(json!({"changed": false, "reason": "cross_tab"})).unwrap();
    assert!(!s.swapped());
}
