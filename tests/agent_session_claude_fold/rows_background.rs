//! Table rows: background work.

use super::*;

pub(crate) const BACKGROUND: &[Row] = &[
    // -- background work -------------------------------------------------------------------
    Row {
        rule: "bg: a turn that ended with its background Bash still running is waiting",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash_background(B + M, "b1");
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: a background Bash whose process is gone does not count",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash_background(B + M, "b1");
            B + 30.0 * M
        },
        live: no_tasks,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: an unchecked task counts up to BG_CAP after it started",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash_background(B + M, "b1");
            B + M + 1.0 + BG_CAP - 10.0
        },
        live: unknown,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: an unchecked task past BG_CAP no longer counts",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash_background(B + M, "b1");
            B + M + 1.0 + BG_CAP + 10.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a terminal notice naming the task ends it",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            let tx = &mut sc.agents.get_mut("a1").unwrap().0;
            let call = tx.bash_background(B + M, "b1");
            tx.notify(B + 2.0 * M, "b1", &call, "completed", &["attachment"]);
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a successful TaskStop ends it",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            let tx = &mut sc.agents.get_mut("a1").unwrap().0;
            tx.bash_background(B + M, "b1");
            tx.task_stop(B + 2.0 * M, "b1", false);
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a failed TaskStop does not end it",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            let tx = &mut sc.agents.get_mut("a1").unwrap().0;
            tx.bash_background(B + M, "b1");
            tx.task_stop(B + 2.0 * M, "b1", true);
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: a Monitor runs until its timeout plus MONITOR_SLACK",
        agent: None,
        build: |sc| {
            sc.main.monitor(B, "m1", 600_000, false);
            B + 1.0 + 600.0 + MONITOR_SLACK - 1.0
        },
        live: no_tasks,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: a Monitor past its timeout has expired",
        agent: None,
        build: |sc| {
            sc.main.monitor(B, "m1", 600_000, false);
            B + 1.0 + 600.0 + MONITOR_SLACK + 1.0
        },
        live: no_tasks,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a persistent Monitor never times out",
        agent: None,
        build: |sc| {
            sc.main.monitor(B, "m1", 600_000, true);
            B + 3.0 * 3600.0
        },
        live: no_tasks,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: a Monitor's [Monitor expired event ends it",
        agent: None,
        build: |sc| {
            sc.main.monitor(B, "m1", 600_000, false);
            sc.main.monitor_expired(B + 2.0 * M, "m1");
            B + 3.0 * M
        },
        live: no_tasks,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a silent parent whose only live descendant waits on its background work is running",
        agent: Some("p1"),
        build: |sc| {
            bg_agent(sc, "p1", "parent");
            let inner = sc.tx("p1").launch(B + M, "c1", "child", true);
            sc.agent(
                "c1",
                json!({"description": "child", "toolUseId": inner, "spawnDepth": 2}),
                "meta_nested",
                None,
            )
            .prompt(B + M, "brief");
            sc.tx("c1").bash_background(B + 2.0 * M, "b1");
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Run, true),
    },
];
