//! Table rows, rule 3: no standing evidence, liveness.

use super::*;

pub(crate) const LIVENESS: &[Row] = &[
    // -- 3. no standing evidence: liveness --------------------------------------------------
    Row {
        rule: "3: written within ACTIVE is running",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + 10.0 * M, "work", false);
            B + 10.0 * M + 1.0 + ACTIVE - 1.0
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "3: a tool call in flight keeps it running up to PENDING_CAP",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents.get_mut("a1").unwrap().0.bash(B + M, "long", true);
            B + M + PENDING_CAP - 1.0
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "3: an abandoned call past PENDING_CAP no longer keeps it running",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents.get_mut("a1").unwrap().0.bash(B + M, "long", true);
            B + M + PENDING_CAP + 1.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: a background agent not notified yet is running within ASYNC_QUIET (inferred)",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + M, "work", false);
            B + M + 1.0 + ASYNC_QUIET - 1.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
    Row {
        rule: "3: past ASYNC_QUIET a silent background agent ended silently (done, inferred)",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + M, "work", false);
            B + M + 1.0 + ASYNC_QUIET + 1.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: a foreground agent whose spawning call has no result yet is running",
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
            B + 40.0 * M
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "3: a handback ends the agent (recorded, not inferred)",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents.get_mut("a1").unwrap().0.handback(B + M);
            B + M + PENDING_CAP + 10.0 // the handback call itself never gets a result
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "3: an idle fork is done (inferred)",
        agent: Some("k1"),
        build: |sc| {
            let call = sc.main.launch(B, "k1", "fork", true);
            sc.agent(
                "k1",
                json!({"description": "fork", "toolUseId": call}),
                "meta_fork",
                None,
            )
            .bash(B + M, "work", false);
            B + M + ACTIVE + 10.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: an agent with no records yet, launched within ASYNC_QUIET, is running (inferred)",
        agent: Some("a1"),
        build: |sc| {
            let call = sc.main.launch(B, "a1", "x", true);
            sc.agent(
                "a1",
                json!({"description": "x", "toolUseId": call}),
                "meta_background",
                None,
            );
            B + ASYNC_QUIET - 10.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
    Row {
        rule: "3: an agent with no records, launched long ago, ended silently",
        agent: Some("a1"),
        build: |sc| {
            let call = sc.main.launch(B, "a1", "x", true);
            sc.agent(
                "a1",
                json!({"description": "x", "toolUseId": call}),
                "meta_background",
                None,
            );
            B + ASYNC_QUIET + 10.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: a silent agent with a running child is running (waiting on it)",
        agent: Some("p1"),
        build: |sc| {
            bg_agent(sc, "p1", "parent");
            let tx = &mut sc.agents.get_mut("p1").unwrap().0;
            let call = tx.launch(B + M, "c1", "child", true);
            let c = sc.agent("c1", json!({"description": "child", "toolUseId": call, "parentAgentId": "p1", "spawnDepth": 2}), "meta_nested", None);
            c.bash(B + 60.0 * M, "work", false);
            B + 60.0 * M + 10.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
];
