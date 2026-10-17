//! Status answers outside the table: resumes, end times, liveness questions, ties, deep chains.

use super::*;

#[test]
fn a_resume_is_counted_and_restarts_the_clock() {
    let mut sc = Sc::new("resume-count");
    let call = bg_agent(&mut sc, "a1", "x");
    sc.main
        .notify(B + 2.0 * M, "a1", &call, "failed", &["queue"]);
    sc.main.resume(B + 3.0 * M, "a1");
    let s = sc.load();
    assert_eq!(s.session().evidence().resumes("a1").len(), 1);
    let st = s
        .session()
        .status_of("a1", Ts(B + 3.0 * M + 30.0), &NoProcs);
    assert_eq!((st.status, st.ended), (Status::Run, None));
}

#[test]
fn ended_at_is_the_evidence_time() {
    let mut sc = Sc::new("ended-at");
    let call = bg_agent(&mut sc, "a1", "x");
    sc.main
        .notify(B + 3.0 * M, "a1", &call, "completed", &["queue"]);
    let s = sc.load();
    let st = s.session().status_of("a1", Ts(B + 30.0 * M), &NoProcs);
    assert_eq!(st.ended, Some(Ts(B + 3.0 * M)));
}

#[test]
fn liveness_is_only_asked_about_what_the_rules_need() {
    struct Panics;
    impl Liveness for Panics {
        fn process_alive(&self, _: &ProcessIdentity) -> Option<bool> {
            panic!("no hook records: no process check")
        }
        fn task_alive(&self, _: &str) -> Option<bool> {
            panic!("no background work: no task check")
        }
    }
    let mut sc = Sc::new("no-checks");
    bg_agent(&mut sc, "a1", "x");
    sc.agents
        .get_mut("a1")
        .unwrap()
        .0
        .bash(B + M, "work", false);
    let s = sc.load();
    assert_eq!(
        s.session()
            .status_of("a1", Ts(B + 10.0 * M), &Panics)
            .status,
        Status::Run
    );
}

/// Terminal evidence at one instant: Python's `max(evidence)` compares the
/// status strings, `cxl` < `done` < `fail`, whatever order they were read in.
#[test]
fn evidence_at_one_instant_ties_like_pythons_status_strings() {
    let cases = [
        (["killed", "completed"], Status::Done),
        (["completed", "failed"], Status::Failed),
        (["killed", "failed"], Status::Failed),
    ];
    for (i, (pair, want)) in cases.into_iter().enumerate() {
        for order in [[pair[0], pair[1]], [pair[1], pair[0]]] {
            let mut sc = Sc::new(&format!("tie{i}"));
            let call = bg_agent(&mut sc, "a1", "x");
            for status in order {
                sc.main.notify(B + 2.0 * M, "a1", &call, status, &["queue"]);
            }
            let s = sc.load();
            let st = s.session().status_of("a1", Ts(B + 3.0 * M), &NoProcs);
            assert_eq!(
                (st.status, st.ended),
                (want, Some(Ts(B + 2.0 * M))),
                "{order:?}"
            );
            let _ = std::fs::remove_dir_all(&sc.root);
        }
    }
    // a notice and a foreground acknowledgement at one instant: (t, "fail") > (t, "done")
    let mut sc = Sc::new("tie-ack");
    let call = sc.main.launch(B, "f1", "fg", false);
    sc.agent(
        "f1",
        json!({"description": "fg", "toolUseId": call}),
        "meta_background",
        None,
    )
    .prompt(B, "brief");
    sc.main.foreground_done(B + 2.0 * M, &call, "f1", false);
    sc.main
        .notify(B + 2.0 * M, "f1", &call, "failed", &["queue"]);
    let s = sc.load();
    let st = s.session().status_of("f1", Ts(B + 3.0 * M), &NoProcs);
    assert_eq!(
        (st.status, st.ended),
        (Status::Failed, Some(Ts(B + 2.0 * M)))
    );
}

/// A `parentAgentId` chain far deeper than Claude Code nests does not
/// exhaust the stack: the running-descendant search stops at NESTING_CAP.
#[test]
fn a_deep_parent_chain_is_answered() {
    use herdr_agent_kit::agent_session::claude::fold::NESTING_CAP;
    let mut s =
        herdr_agent_kit::agent_session::claude::Session::new(SessionId::parse(SID).unwrap());
    let n = 50_000;
    for i in 0..n {
        let meta = if i == 0 {
            Map::new()
        } else {
            json!({"parentAgentId": format!("a{}", i - 1)})
                .as_object()
                .unwrap()
                .clone()
        };
        s.set_meta(&format!("a{i}"), meta);
    }
    let st = s.status_of("a0", Ts(B), &NoProcs);
    assert_eq!((st.status, st.inferred), (Status::Done, true));
    // within the cap, a running descendant still keeps its ancestors running
    let mut s =
        herdr_agent_kit::agent_session::claude::Session::new(SessionId::parse(SID).unwrap());
    for i in 0..NESTING_CAP {
        let mut meta = json!({"parentAgentId": format!("a{}", i.saturating_sub(1))});
        if i == 0 {
            meta = json!({});
        }
        s.set_meta(&format!("a{i}"), meta.as_object().unwrap().clone());
    }
    let leaf = Owner::Agent(format!("a{}", NESTING_CAP - 1));
    s.file_mut(leaf).last_ts = Some(Ts(B));
    assert_eq!(s.status_of("a0", Ts(B + 1.0), &NoProcs).status, Status::Run);
}
