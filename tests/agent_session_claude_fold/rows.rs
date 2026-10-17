//! The status-rules table: a [`Row`], the process checks rows use, and the table test.

use super::*;

/// One row: how to build the session, the agent asked about, the instant,
/// the process checks, and the expected `(status, inferred)`.
pub(crate) struct Row {
    pub(crate) rule: &'static str,
    /// The agent asked about; `None`: main's own background work.
    pub(crate) agent: Option<&'static str>,
    pub(crate) build: fn(&mut Sc) -> f64,
    pub(crate) live: fn() -> FakeLiveness,
    pub(crate) want: (Status, bool),
}

pub(crate) fn unknown() -> FakeLiveness {
    FakeLiveness::default()
}

pub(crate) fn no_tasks() -> FakeLiveness {
    FakeLiveness::default().tasks::<&str>(&[])
}

pub(crate) fn task_b1_running() -> FakeLiveness {
    FakeLiveness::default().tasks(&["b1"])
}

pub(crate) fn hook_process_gone() -> FakeLiveness {
    FakeLiveness::default().process(
        ProcessIdentity::new(4242, "s1", Some("pid:[1]".into())),
        false,
    )
}

/// Two Claude processes: 4242 is gone, 5555 lives.
pub(crate) fn first_hook_process_gone() -> FakeLiveness {
    hook_process_gone().process(
        ProcessIdentity::new(5555, "s9", Some("pid:[1]".into())),
        true,
    )
}

pub(crate) fn hook_process_alive() -> FakeLiveness {
    FakeLiveness::default().process(
        ProcessIdentity::new(4242, "s1", Some("pid:[1]".into())),
        true,
    )
}

/// panorama `test_a_relaunched_run_abandons_the_old_instances_unfinished_agents`:
/// run wf_1 launched at B, relaunched at B+10m; "old" started under the
/// first instance, "new" under the second.
pub(crate) fn relaunched_run(sc: &mut Sc) -> f64 {
    sc.main.workflow(B, "wf_1", "task1");
    sc.journal("wf_1", "launched", "", "");
    sc.journal("wf_1", "started", "old", "Build");
    sc.journal("wf_1", "started", "new", "Build");
    for (id, start) in [("old", B + M), ("new", B + 11.0 * M)] {
        sc.agent(
            id,
            json!({"description": id, "workflowPhase": "Build"}),
            "meta_workflow",
            Some("wf_1"),
        )
        .prompt(start, "brief");
    }
    sc.main.workflow(B + 10.0 * M, "wf_1", "task2"); // resumed: a new launch of the same run
    B + 12.0 * M
}

/// The table, in the order its sections were written.
const ROWS: &[&[Row]] = &[
    rows_evidence::EVIDENCE,
    rows_reopen::REOPEN,
    rows_liveness::LIVENESS,
    rows_background::BACKGROUND,
    rows_hooks::HOOKS,
    rows_review::REVIEW,
];

#[test]
pub(crate) fn the_status_rules() {
    let mut failures = Vec::new();
    for (i, row) in ROWS.iter().flat_map(|rows| rows.iter()).enumerate() {
        let mut sc = Sc::new(&format!("row{i}"));
        let now = (row.build)(&mut sc);
        let scanner = sc.load();
        let live = (row.live)();
        let got = match row.agent {
            Some(agent) => {
                let s = scanner.session().status_of(agent, Ts(now), &live);
                (s.status, s.inferred)
            }
            // no agent: the row is about main's own background work
            None => {
                let waiting = !scanner
                    .session()
                    .waiting_bg(&Owner::Main, Ts(now), &live)
                    .is_empty();
                (if waiting { Status::Bg } else { Status::Done }, !waiting)
            }
        };
        if got != row.want {
            failures.push(format!("{}: got {got:?}, want {:?}", row.rule, row.want));
        }
        let _ = std::fs::remove_dir_all(&sc.root);
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
