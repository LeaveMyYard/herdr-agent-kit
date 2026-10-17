//! Table rows: hook records.

use super::*;

pub(crate) const HOOKS: &[Row] = &[
    // -- hooks -----------------------------------------------------------------------------
    Row {
        rule: "hooks: a recorded stop is an ending",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            sc.hook("stop", "a1", B + 2.0 * M, 4242, "s1");
            B + 3.0 * M
        },
        live: hook_process_alive,
        want: (Status::Done, false),
    },
    Row {
        rule: "hooks: started, never stopped, its Claude process gone: lost",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            B + 30.0 * M
        },
        live: hook_process_gone,
        want: (Status::Lost, false),
    },
    Row {
        rule: "hooks: started, never stopped, its process still alive: not lost",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            B + 30.0 * M
        },
        live: hook_process_alive,
        want: (Status::Done, true),
    },
    Row {
        rule: "hooks: a later record from another Claude process (a resume): not lost",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            sc.hook("start", "other", B + M, 5555, "s9");
            B + 30.0 * M
        },
        live: hook_process_gone,
        want: (Status::Done, true),
    },
];
