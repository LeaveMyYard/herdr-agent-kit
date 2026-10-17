//! Table rows, rule 1: terminal evidence.

use super::*;

pub(crate) const EVIDENCE: &[Row] = &[
    // -- 1. terminal evidence --------------------------------------------------------------
    Row {
        rule: "1: a completed notice from the queue carrier alone ends the agent",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + 2.0 * M, "look", false);
            sc.main
                .notify(B + 3.0 * M, "a1", &call, "completed", &["queue"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a notice from the attachment carrier counts",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 3.0 * M, "a1", &call, "completed", &["attachment"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a notice in a plain user string counts (older sessions)",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 3.0 * M, "a1", &call, "completed", &["user"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a notice in the spawning agent's own file counts",
        agent: Some("c1"),
        build: |sc| {
            bg_agent(sc, "p1", "parent");
            let tx = &mut sc.agents.get_mut("p1").unwrap().0;
            let call = tx.launch(B + M, "c1", "child", true);
            let tx = sc.agent("c1", json!({"description": "child", "toolUseId": call, "parentAgentId": "p1", "spawnDepth": 2}), "meta_nested", None);
            tx.prompt(B + M, "brief");
            sc.agents.get_mut("p1").unwrap().0.notify(
                B + 3.0 * M,
                "c1",
                &call,
                "completed",
                &["attachment"],
            );
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: failed is fail",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "failed", &["queue", "attachment"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Failed, false),
    },
    Row {
        rule: "1: killed is cxl",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.agents.get_mut("a1").unwrap().0.bash(B + M, "work", true);
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "killed", &["queue", "attachment"]);
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: stopped is cxl",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "stopped", &["queue"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: the latest notice wins (failed, resumed, completed: done)",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "failed", &["queue"]);
            sc.main.resume(B + 3.0 * M, "a1");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + 4.0 * M, "retry", false);
            sc.main
                .notify(B + 5.0 * M, "a1", &call, "completed", &["queue"]);
            B + 6.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a foreground spawn's completed acknowledgement ends it",
        agent: Some("f1"),
        build: |sc| {
            let call = sc.main.launch(B, "f1", "fg", false);
            sc.agent(
                "f1",
                json!({"description": "fg", "toolUseId": call}),
                "meta_background",
                None,
            )
            .prompt(B, "brief");
            sc.main.foreground_done(B + 2.0 * M, &call, "f1", false);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: an erroring foreground acknowledgement is fail",
        agent: Some("f1"),
        build: |sc| {
            let call = sc.main.launch(B, "f1", "fg", false);
            sc.agent(
                "f1",
                json!({"description": "fg", "toolUseId": call}),
                "meta_background",
                None,
            )
            .prompt(B, "brief");
            sc.main.foreground_done(B + 2.0 * M, &call, "f1", true);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Failed, false),
    },
    Row {
        rule: "1: a workflow journal result is done",
        agent: Some("w1"),
        build: |sc| {
            sc.main.workflow(B, "wf_1", "t_wf");
            sc.agent(
                "w1",
                json!({"description": "step", "workflowPhase": "build"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", false);
            sc.journal("wf_1", "started", "w1", "build");
            sc.journal("wf_1", "result", "w1", "build");
            B + 2.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a workflow journal failure is fail",
        agent: Some("w1"),
        build: |sc| {
            sc.main.workflow(B, "wf_1", "t_wf");
            sc.agent(
                "w1",
                json!({"description": "step"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", false);
            sc.journal("wf_1", "failed", "w1", "build");
            B + 2.0 * M
        },
        live: unknown,
        want: (Status::Failed, false),
    },
    Row {
        rule: "1: the end of the agent's killed workflow run is cxl",
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
                .notify(B + 2.0 * M, "t_wf", "toolu_wf", "killed", &["queue"]);
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: a killed run whose launch was compacted away: its summary ends it (cxl)",
        agent: Some("w1"),
        build: |sc| {
            sc.agent(
                "w1",
                json!({"description": "step"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", false);
            sc.summary("wf_1", "killed");
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
];
