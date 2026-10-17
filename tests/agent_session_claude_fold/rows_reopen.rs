//! Table rows, rule 2: evidence overturned.

use super::*;

pub(crate) const REOPEN: &[Row] = &[
    // -- 2. evidence overturned ------------------------------------------------------------
    Row {
        rule: "2: a later SendMessage resume reopens a finished agent",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.main.resume(B + 3.0 * M, "a1");
            B + 4.0 * M
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "2: a resumed agent quiet for less than ASYNC_QUIET is still working (inferred)",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.main.resume(B + 3.0 * M, "a1");
            B + 3.0 * M + 1.0 + ACTIVE + 1.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
    Row {
        rule: "2: the agent's own activity more than REOPEN_MARGIN after its notice reopens it",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .text(B + 2.0 * M + REOPEN_MARGIN + 10.0, "more");
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "2: a final line within REOPEN_MARGIN of the notice does not reopen it",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .text(B + 2.0 * M + REOPEN_MARGIN - 2.0, "report");
            B + 2.0 * M + 30.0
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "reopen: a resume at exactly the ending's time does not overturn it",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.main.resume(B + 2.0 * M - 1.0, "a1"); // its result lands at B + 2m
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
];
