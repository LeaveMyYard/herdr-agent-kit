//! Table rows from the review rounds: one row per branch.

use super::*;

pub(crate) const REVIEW: &[Row] = &[
    // -- review round 1 (second review) and the final pass: one row per branch -------------
    Row {
        rule: "1: the end of the agent's failed workflow run is cxl too",
        agent: Some("w1"),
        build: |sc| {
            sc.main.workflow(B, "wf_1", "t_wf");
            sc.agent(
                "w1",
                json!({"description": "step"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", true);
            sc.main
                .notify(B + 2.0 * M, "t_wf", "toolu_wf", "failed", &["queue"]);
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: a failed run whose launch was compacted away: its summary ends it (cxl)",
        agent: Some("w1"),
        build: |sc| {
            sc.agent(
                "w1",
                json!({"description": "step"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", false);
            sc.summary("wf_1", "failed");
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: a relaunched run abandons the old instance's unfinished agent (cxl)",
        agent: Some("old"),
        build: relaunched_run,
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: a relaunched run's new agent runs while the run is live",
        agent: Some("new"),
        build: |sc| {
            relaunched_run(sc);
            B + 14.0 * M
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "2: a recorded hook start after the evidence reopens the agent",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.hook("start", "a1", B + 5.0 * M, 4242, "s1");
            B + 5.5 * M
        },
        live: hook_process_alive,
        want: (Status::Run, false),
    },
    Row {
        rule: "3: a foreground spawning call with any result (an error with no status) no longer keeps it running",
        agent: Some("f1"),
        build: |sc| {
            let call = sc.main.launch(B, "f1", "fg", false);
            sc.agent(
                "f1",
                json!({"description": "fg", "toolUseId": call, "requestShape": "foreground"}),
                "meta_background",
                None,
            )
            .bash(B + M, "work", false);
            sc.main.spawn_error(B + 2.0 * M, &call, "f1");
            B + 40.0 * M
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: an agent with no records and no acknowledgement is aged from its spawning call",
        agent: Some("f1"),
        build: |sc| {
            let call = sc.main.launch(B, "f1", "fg", false);
            sc.agent(
                "f1",
                json!({"description": "fg", "toolUseId": call, "requestShape": "foreground"}),
                "meta_background",
                None,
            );
            B + ASYNC_QUIET - 10.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
    Row {
        rule: "hooks: a notice up to REOPEN_MARGIN before a recorded stop says how it ended",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M - 3.0, "a1", &call, "failed", &["queue"]);
            sc.hook("stop", "a1", B + 2.0 * M, 4242, "s1");
            B + 3.0 * M
        },
        live: hook_process_alive,
        want: (Status::Failed, false),
    },
    Row {
        rule: "hooks: of two starts at one instant the first decides (its process gone: lost)",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            sc.hook("start", "a1", B, 5555, "s9");
            B + 30.0 * M
        },
        live: first_hook_process_gone,
        want: (Status::Lost, false),
    },
];
