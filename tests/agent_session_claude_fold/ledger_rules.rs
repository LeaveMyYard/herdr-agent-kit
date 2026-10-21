//! Status rules panorama's tests check that the table did not (herdr-agent-kit#29): ported
//! from panorama's `tests/test_background.py` and `tests/test_scan_session.py` (main
//! `83590dc`), each test named for the Python one, its assertions the Python test's.

use super::*;

/// `test_background.py::test_the_process_check_decides_when_it_can`: a task the process check
/// finds alive has no age cap; one whose shell is gone without a notice is over.
#[test]
fn the_process_check_decides_when_it_can() {
    let mut sc = Sc::new("process-check");
    sc.main.bash_background(B, "bq9");
    sc.main.bash_background(B, "bq10");
    let s = sc.load();
    let live = FakeLiveness::default().tasks(&["bq9"]);
    let at = Ts(B + 30.0 * 3600.0);
    const { assert!(30.0 * 3600.0 > BG_CAP, "past the cap of an unchecked task") };
    let waiting: Vec<&str> = s
        .session()
        .background()
        .waiting(&Owner::Main, at, &live)
        .map(|l| l.task.as_str())
        .collect();
    assert_eq!(waiting, ["bq9"]);
}

/// `test_background.py::test_an_agent_notified_complete_is_done_whatever_it_left_behind`: the
/// completion guard: a finished agent's leftover loops never make it waiting.
#[test]
fn an_agent_notified_complete_is_done_whatever_it_left_behind() {
    let mut sc = Sc::new("completion-guard");
    let call = sc.launched(B, "a2", true, json!({}));
    sc.tx("a2").bash_background(B + M, "ab2");
    sc.main.notify(
        B + 5.0 * M,
        "a2",
        &call,
        "completed",
        &["queue", "attachment"],
    );
    let s = sc.load();
    let live = FakeLiveness::default().tasks(&["ab2"]);
    assert_eq!(
        s.session().status_of("a2", Ts(B + 30.0 * M), &live).status,
        Status::Done
    );
}

/// `test_scan_session.py::test_foreground_agent_completes_with_its_result`.
#[test]
fn a_foreground_agent_completes_with_its_result() {
    let mut sc = Sc::new("foreground-result");
    let call = sc.launched(B, "f1", false, json!({"requestShape": "foreground"}));
    sc.tx("f1").bash(B + M, "think", false);
    sc.main
        .foreground_counted(B + 2.0 * M, &call, "f1", 9, false);
    let call2 = sc.main.launch(B + 3.0 * M, "f2", "plan again", false);
    sc.main
        .foreground_counted(B + 4.0 * M, &call2, "f2", 1, true);
    let s = sc.load();
    let s = s.session();
    assert_eq!(
        s.status_of("f1", Ts(B + 2.5 * M), &NoProcs).status,
        Status::Done
    );
    assert_eq!(s.parentage().ack("f1").and_then(|a| a.tool_count), Some(9));
    assert_eq!(
        s.status_of("f2", Ts(B + 4.5 * M), &NoProcs).status,
        Status::Failed
    );
}

/// `test_scan_session.py::test_h1_foreground_agent_runs_while_its_spawning_call_is_open`: the
/// open call is ground truth, up to PENDING_CAP like an abandoned call.
#[test]
fn a_foreground_agent_runs_while_its_spawning_call_is_open() {
    let mut sc = Sc::new("foreground-open");
    sc.launched(B, "f1", false, json!({"requestShape": "foreground"}));
    sc.tx("f1").bash(B + M, "look", false);
    let s = sc.load();
    let at = |t: f64| s.session().status_of("f1", Ts(t), &NoProcs).status;
    assert_eq!(at(B + 5.0 * M), Status::Run); // 4 min of thinking
    assert_eq!(at(B + M + PENDING_CAP + 60.0), Status::Done);
}

/// `test_scan_session.py::test_m1_a_huge_last_line_keeps_the_timestamp_and_the_call_in_flight`.
#[test]
fn a_huge_last_line_keeps_the_timestamp_and_the_call_in_flight() {
    let mut sc = Sc::new("huge-line");
    sc.launched(B, "f1", false, json!({"requestShape": "foreground"}));
    sc.tx("f1").bash(B + M, &"x".repeat(100_000), true); // a >64 KB tool_use line, still running
    let s = sc.load();
    let info = &s.session().file_of("f1").expect("read").info;
    assert_eq!((info.last_ts, info.pending.len()), (Some(Ts(B + M)), 1));
    assert_eq!(
        s.session()
            .status_of("f1", Ts(B + 5.0 * M), &NoProcs)
            .status,
        Status::Run
    );
}

/// `test_scan_session.py::test_l3_a_failed_resume_does_not_reopen`.
#[test]
fn a_failed_resume_does_not_reopen() {
    let mut sc = Sc::new("failed-resume");
    let call = sc.launched(B, "a1", true, json!({}));
    sc.main
        .notify(B + 2.0 * M, "a1", &call, "failed", &["queue", "attachment"]);
    sc.main.resume(B + 3.0 * M, "a1");
    let last = sc.main.lines.last_mut().expect("the resume's result");
    let mut line: Value = serde_json::from_str(last).unwrap();
    line["toolUseResult"]["success"] = json!(false);
    *last = line.to_string();
    let s = sc.load();
    assert_eq!(
        s.session()
            .status_of("a1", Ts(B + 3.5 * M), &NoProcs)
            .status,
        Status::Failed
    );
}

/// `test_scan_session.py::test_l1_a_bad_record_does_not_break_the_session`: list and dict
/// timestamps, a list `resumedAgentId`, `spawnDepth: "two"`.
#[test]
fn a_bad_record_does_not_break_the_session() {
    let mut sc = Sc::new("bad-record");
    let call = sc.main.launch(B, "a1", "x", true);
    sc.main.raw(
        json!({"type": "user", "timestamp": ["not", "a", "time"], "message": {"content": "x"}}),
    );
    sc.main.raw(
        json!({"type": "user", "timestamp": {"a": 1}, "toolUseResult": {"resumedAgentId": ["x"]},
                       "message": {"content": [{"type": "tool_result", "tool_use_id": call}]}}),
    );
    sc.main.notify(
        B + 2.0 * M,
        "a1",
        &call,
        "completed",
        &["queue", "attachment"],
    );
    sc.agent(
        "a1",
        json!({"toolUseId": call, "spawnDepth": "two"}),
        "meta_background",
        None,
    )
    .prompt(B, "brief");
    let s = sc.load();
    assert_eq!(
        s.session()
            .status_of("a1", Ts(B + 3.0 * M), &NoProcs)
            .status,
        Status::Done
    );
}

/// `test_scan_session.py::test_a_notice_about_the_same_stop_says_how_it_ended`: a notice just
/// after a recorded stop names how it ended.
#[test]
fn a_notice_about_the_same_stop_says_how_it_ended() {
    let mut sc = Sc::new("stop-notice");
    let call = sc.launched(B, "a1", true, json!({}));
    sc.main.notify(
        B + 2.0 * M + 1.0,
        "a1",
        &call,
        "failed",
        &["queue", "attachment"],
    );
    sc.hook_bare("stop", "a1", B + 2.0 * M);
    let s = sc.load();
    assert_eq!(
        s.session()
            .status_of("a1", Ts(B + 3.0 * M), &NoProcs)
            .status,
        Status::Failed
    );
}

/// `test_scan_session.py::test_l3_a_late_failed_notice_still_wins`: 3 minutes after the stop.
#[test]
fn a_late_failed_notice_still_wins() {
    let mut sc = Sc::new("late-notice");
    let call = sc.launched(B, "a1", true, json!({}));
    sc.main
        .notify(B + 5.0 * M, "a1", &call, "failed", &["queue", "attachment"]);
    sc.hook_bare("stop", "a1", B + 2.0 * M);
    let s = sc.load();
    assert_eq!(
        s.session()
            .status_of("a1", Ts(B + 6.0 * M), &NoProcs)
            .status,
        Status::Failed
    );
}

/// `test_scan_session.py::test_started_never_stopped_and_process_gone_is_lost`, its last part
/// (the table has the others): a start recorded without its process is never lost.
#[test]
fn a_start_without_its_process_is_never_lost() {
    let mut sc = Sc::new("start-no-process");
    sc.launched(B, "a1", true, json!({}));
    sc.tx("a1").bash(B + M, "work", false);
    sc.hook_bare("start", "a1", B + 1.0);
    let s = sc.load();
    let st = s
        .session()
        .status_of("a1", Ts(B + 2.0 * M + ASYNC_QUIET), &hook_process_gone());
    assert_eq!(st.status, Status::Done);
}
