//! What a session's files tell, and the wire rules, as panorama's tests check them and no kit
//! test did (herdr-agent-kit#29): ported from panorama's `tests/test_scan_session.py` and
//! `tests/test_scan_wire.py` (main `83590dc`), each test named for the Python one.

use super::*;
use herdr_agent_kit::agent_session::claude::Line;
use herdr_agent_kit::agent_session::claude::wire::parse_notice;

/// `test_scan_session.py::test_the_fold_does_not_depend_on_record_order`: main's records in
/// any order give the same statuses (Python shuffles with five seeds; here five fixed
/// permutations: reversed, rotations, odd-before-even).
#[test]
fn the_fold_does_not_depend_on_record_order() {
    let mut sc = Sc::new("record-order");
    sc.main.prompt(B, "go");
    let call = sc.main.launch(B + 1.0, "a1", "x", true);
    sc.main
        .notify(B + 2.0 * M, "a1", &call, "failed", &["queue", "attachment"]);
    sc.main.resume(B + 3.0 * M, "a1");
    sc.main.notify(
        B + 5.0 * M,
        "a1",
        &call,
        "completed",
        &["queue", "attachment"],
    );
    let call2 = sc.main.launch(B + 6.0 * M, "a2", "y", true);
    sc.main.notify(
        B + 7.0 * M,
        "a2",
        &call2,
        "killed",
        &["queue", "attachment"],
    );
    sc.agent("a1", json!({"toolUseId": call}), "meta_background", None)
        .prompt(B + 1.0, "brief");
    sc.agent("a2", json!({"toolUseId": call2}), "meta_background", None)
        .prompt(B + 6.0 * M, "brief");
    let statuses = |s: &Scanner| {
        ["a1", "a2"].map(|a| {
            let st = s.session().status_of(a, Ts(B + 9.0 * M), &NoProcs);
            (st.status, st.ended, st.inferred)
        })
    };
    let want = statuses(&sc.load());
    assert_eq!((want[0].0, want[1].0), (Status::Done, Status::Cancelled));
    let lines = sc.main.lines.clone();
    let n = lines.len();
    let orders: [Vec<usize>; 5] = [
        (0..n).rev().collect(),
        (0..n).map(|i| (i + 3) % n).collect(),
        (0..n).map(|i| (i + n / 2) % n).collect(),
        (0..n)
            .filter(|i| i % 2 == 1)
            .chain((0..n).filter(|i| i % 2 == 0))
            .collect(),
        (0..n).map(|i| (i * 7 + 2) % n).collect(),
    ];
    for order in orders {
        let mut seen = order.clone();
        seen.sort_unstable();
        if seen != (0..n).collect::<Vec<_>>() {
            continue; // `i * 7 + 2` is a permutation only when 7 and n are coprime
        }
        sc.main.lines = order.iter().map(|&i| lines[i].clone()).collect();
        assert_eq!(statuses(&sc.load()), want, "{order:?}");
    }
}

/// `test_scan_session.py::test_titles_and_prompts`: a legacy prompt (no origin) counts, a
/// notice in a user string does not.
#[test]
fn titles_and_prompts() {
    let mut sc = Sc::new("titles-prompts");
    sc.main.title("custom_title", "billing-api");
    sc.main.title("agent_name", "billing");
    sc.main.prompt(B, "first task");
    sc.main.prompt_legacy(B + 5.0 * M, "legacy prompt");
    sc.main
        .notify(B + 6.0 * M, "zz", "toolu_z", "completed", &["user"]);
    let s = sc.load();
    let c = s.session().conversation();
    let t = c.titles();
    assert_eq!(
        (t.custom.as_deref(), t.agent.as_deref(), t.ai.as_deref()),
        (Some("billing-api"), Some("billing"), None) // Python compares the whole dict: no ai title
    );
    let texts: Vec<&str> = c.prompts().map(|p| p.text).collect();
    assert_eq!(texts, ["first task", "legacy prompt"]);
    let before = c
        .prompt_before(Some(Ts(B + 6.0 * M)))
        .map(|p| (p.index, p.text));
    assert_eq!(before, Some((1, "legacy prompt")));
}

/// `test_scan_session.py::test_l4_tool_calls_count_only_the_agents_own_calls`: a tool_use in
/// an output does not count.
#[test]
fn tool_calls_count_only_the_agents_own_calls() {
    let mut sc = Sc::new("tool-calls");
    sc.launched(B, "a1", true, json!({}));
    sc.tx("a1").bash_output(
        B + M,
        "read a transcript",
        r#"{"type":"tool_use"} {"type":"tool_use"}"#,
    );
    let s = sc.load();
    assert_eq!(s.session().file_of("a1").expect("read").tool_calls, 1);
}

/// `test_scan_wire.py::test_notice_statuses`, its `weird` status: no status.
#[test]
fn notice_statuses() {
    let status = |s: &str| {
        parse_notice(&notification("a", "t", s, 0))
            .expect("a notice")
            .status
    };
    let got = ["completed", "failed", "error", "killed", "stopped", "weird"].map(status);
    use herdr_agent_kit::agent_session::claude::wire::Ending::{Cancelled, Done, Failed};
    let want = [
        Some(Done),
        Some(Failed),
        Some(Failed),
        Some(Cancelled),
        Some(Cancelled),
        None,
    ];
    assert_eq!(got, want);
}

/// `test_scan_wire.py::test_human_prompt_rule`.
#[test]
fn human_prompt_rule() {
    let is_human = |rec: &Map<String, Value>| {
        Line::parse(serde_json::to_string(rec).unwrap().as_bytes())
            .expect("a line")
            .is_human_prompt()
    };
    let mut human = tpl("user_prompt_human");
    human["message"]["content"] = json!("do it");
    assert!(is_human(&human));
    human.insert("origin".into(), json!({"kind": "task-notification"}));
    assert!(!is_human(&human));
    let mut legacy = tpl("user_prompt");
    legacy.remove("origin");
    legacy["message"]["content"] = json!("old style");
    assert!(is_human(&legacy));
    legacy["message"]["content"] = json!(notification("a", "t", "completed", 0));
    assert!(!is_human(&legacy));
}
