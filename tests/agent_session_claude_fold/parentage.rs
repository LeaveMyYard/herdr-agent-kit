//! Parentage: spawning calls, forks, children and kept scanners.

use super::*;

#[test]
fn parentage_follows_the_spawning_call() {
    let mut sc = Sc::new("parentage");
    bg_agent(&mut sc, "p1", "parent");
    let call = sc
        .agents
        .get_mut("p1")
        .unwrap()
        .0
        .launch(B + M, "c1", "child", true);
    sc.agent(
        "c1",
        json!({"description": "child", "toolUseId": call, "spawnDepth": 2}),
        "meta_nested",
        None,
    )
    .prompt(B + M, "brief");
    // a meta that names the parent, whose spawning call was never read
    sc.agent(
        "o1",
        json!({"description": "orphan", "toolUseId": "toolu_gone", "parentAgentId": "p1"}),
        "meta_nested",
        None,
    )
    .prompt(B + M, "brief");
    let s = sc.load();
    let sess = s.session();
    assert_eq!(sess.parentage().parent_of("p1"), Some(Owner::Main));
    assert_eq!(
        sess.parentage().parent_of("c1"),
        Some(Owner::Agent("p1".into()))
    );
    assert_eq!(
        sess.parentage().parent_of("o1"),
        Some(Owner::Agent("p1".into()))
    );
    assert_eq!(
        sess.parentage().children(&Owner::Agent("p1".into())),
        vec!["c1", "o1"]
    );
}

#[test]
fn a_fork_repeating_its_parents_history_never_takes_the_spawning_call() {
    let mut sc = Sc::new("fork-history");
    let call = bg_agent(&mut sc, "a1", "x");
    // the fork's file repeats main's history, the spawning call included, later
    let tx = sc.agent(
        "k1",
        json!({"description": "fork", "toolUseId": "toolu_k"}),
        "meta_fork",
        None,
    );
    let rec = tool_block(
        tpl("sub_assistant_agent"),
        "Agent",
        &call,
        json!({"description": "x", "run_in_background": true}),
    );
    tx.add(rec, B + 5.0 * M);
    let s = sc.load();
    assert_eq!(
        s.session()
            .parentage()
            .spawn_of("a1")
            .map(|sp| sp.owner.clone()),
        Some(Owner::Main)
    );
    assert_eq!(s.session().parentage().parent_of("a1"), Some(Owner::Main));
}

/// panorama `test_a_fork_does_not_take_over_its_own_spawning_call`.
#[test]
fn a_fork_does_not_take_over_its_own_spawning_call() {
    let mut sc = Sc::new("fork-own-call");
    let call = sc.main.launch_as(B, "k1", "fork it", true, "fork");
    let copied = sc.main.lines[0].clone();
    let k1 = sc.agent(
        "k1",
        json!({"description": "fork it", "agentType": "fork", "toolUseId": call}),
        "meta_fork",
        None,
    );
    k1.lines.push(copied); // the copied history
    k1.prompt(B + 1.0, "brief");
    let s = sc.load();
    assert_eq!(s.session().parentage().parent_of("k1"), Some(Owner::Main));
    assert!(
        s.session()
            .parentage()
            .children(&Owner::Agent("k1".into()))
            .is_empty()
    );
}

/// panorama `test_n1_a_fork_of_a_subagent_nests_under_it_even_when_only_its_own_copy_is_read`:
/// the fork's own file repeats its spawning call; the real parent's file is
/// skipped (`since`), so only the fork's own copy is read, and it never counts.
#[test]
fn a_fork_of_a_subagent_nests_under_it_when_only_its_own_copy_is_read() {
    let mut sc = Sc::new("fork-own-copy");
    bg_agent(&mut sc, "p1", "parent");
    let inner = sc.tx("p1").launch_as(B + M, "k1", "fork it", true, "fork");
    let copied = sc.tx("p1").lines[1].clone(); // the spawning call
    let k1 = sc.agent(
        "k1",
        json!({"description": "fork it", "agentType": "fork", "toolUseId": inner,
               "spawnDepth": 2, "parentAgentId": "p1"}),
        "meta_fork",
        None,
    );
    k1.lines.push(copied);
    k1.prompt(B + 2.0 * M, "brief");
    let mut s = sc.write_all();
    set_mtime(&sc.agents["p1"].0.path, B); // the parent's file is old: skipped
    s.update(Some(Ts(B + 60.0)));
    assert_eq!(read_ts(&s, "p1"), None);
    let sess = s.session();
    assert_eq!(
        sess.parentage().parent_of("k1"),
        Some(Owner::Agent("p1".into()))
    );
    assert_eq!(
        sess.parentage().children(&Owner::Agent("p1".into())),
        vec!["k1"]
    );
    assert!(
        sess.parentage()
            .children(&Owner::Agent("k1".into()))
            .is_empty()
    );
}

/// Folds `tx`'s lines from `from` on into `s` as `owner`'s records.
fn fold_lines(
    s: &mut herdr_agent_kit::agent_session::claude::Session,
    owner: Owner,
    tx: &Tx,
    from: usize,
) {
    for raw in &tx.lines[from..] {
        let line = herdr_agent_kit::agent_session::claude::Line::parse(raw.as_bytes()).unwrap();
        let when = line.ts();
        s.apply(owner.clone(), &line, when);
    }
}

/// `children` is cached between inputs: each input that can change
/// parentage (`apply`, `set_agent`, `set_meta`), used alone after the cache
/// was filled, shows in the next answer.
#[test]
fn children_follow_every_input_that_changes_parentage() {
    let mut sc = Sc::new("children-cache");
    bg_agent(&mut sc, "p1", "parent");
    let p1 = Owner::Agent("p1".into());
    let mut s =
        herdr_agent_kit::agent_session::claude::Session::new(SessionId::parse(SID).unwrap());
    fold_lines(&mut s, Owner::Main, &sc.main, 0);
    s.set_agent("p1", PathBuf::from("p1.jsonl"), None);
    s.set_meta("p1", sc.agents["p1"].1.clone());
    assert_eq!(s.parentage().parent_of("p1"), Some(Owner::Main));

    // apply: c1 is known with its meta; its spawning call is read later
    let inner = sc.tx("p1").launch(B + M, "c1", "child", false);
    s.set_agent("c1", PathBuf::from("c1.jsonl"), None);
    s.set_meta(
        "c1",
        json!({"toolUseId": inner}).as_object().unwrap().clone(),
    );
    assert!(s.parentage().children(&p1).is_empty());
    let n = sc.agents["p1"].0.lines.len();
    fold_lines(&mut s, p1.clone(), &sc.agents["p1"].0, n - 1);
    assert_eq!(s.parentage().children(&p1), vec!["c1"]);

    // set_meta: o1's meta, empty at first (mid-write), then naming its parent
    s.set_agent("o1", PathBuf::from("o1.jsonl"), None);
    s.set_meta("o1", Map::new());
    assert_eq!(s.parentage().children(&p1), vec!["c1"]);
    s.set_meta(
        "o1",
        json!({"parentAgentId": "p1"}).as_object().unwrap().clone(),
    );
    assert_eq!(s.parentage().children(&p1), vec!["c1", "o1"]);

    // set_agent: d1's spawn and acknowledgement were read before its file appeared
    let n = sc.agents["p1"].0.lines.len();
    sc.tx("p1").launch(B + 2.0 * M, "d1", "later", true);
    fold_lines(&mut s, p1.clone(), &sc.agents["p1"].0, n);
    assert_eq!(s.parentage().children(&p1), vec!["c1", "o1"]);
    s.set_agent("d1", PathBuf::from("d1.jsonl"), None);
    assert_eq!(s.parentage().children(&p1), vec!["c1", "o1", "d1"]); // discovery order
}

/// One scanner across collects (as panorama keeps one per session): a
/// silent parent reads as done until it spawns a child, then runs while the
/// child runs.
#[test]
fn a_kept_scanner_keeps_a_silent_parent_running_while_its_new_child_runs() {
    let mut sc = Sc::new("children-kept");
    bg_agent(&mut sc, "p1", "parent");
    let p1 = Owner::Agent("p1".into());
    let now = B + 60.0 * M;
    let mut s = sc.load();
    let st = s.session().status_of("p1", Ts(now), &NoProcs);
    assert_eq!((st.status, st.inferred), (Status::Done, true));
    assert!(s.session().parentage().children(&p1).is_empty());

    let inner = sc.tx("p1").launch(B + M, "c1", "child", true);
    sc.agent(
        "c1",
        json!({"description": "child", "toolUseId": inner, "spawnDepth": 2}),
        "meta_nested",
        None,
    )
    .prompt(B + M, "brief");
    sc.tx("c1").bash(now - M, "long work", false);
    let _ = sc.write_all(); // the files only; `s` stays the scanner
    s.update(None);
    assert_eq!(s.session().parentage().children(&p1), vec!["c1"]);
    assert_eq!(
        s.session().status_of("c1", Ts(now), &NoProcs).status,
        Status::Run
    );
    assert_eq!(
        s.session().status_of("p1", Ts(now), &NoProcs).status,
        Status::Run
    );
}

/// panorama's `parent_of`: of two non-main owners of the spawning call at
/// the same earliest time (a fork copies its parent's records with their
/// times), the first read wins.
#[test]
fn of_two_earliest_owners_of_a_spawning_call_the_first_read_is_the_parent() {
    for (first, second) in [("p1", "k1"), ("k1", "p1")] {
        let mut sc = Sc::new("parent-tie");
        bg_agent(&mut sc, "p1", "parent");
        let inner = sc.tx("p1").launch(B + M, "c1", "child", false);
        let call_line = sc.tx("p1").lines.last().unwrap().clone();
        let mut s =
            herdr_agent_kit::agent_session::claude::Session::new(SessionId::parse(SID).unwrap());
        s.set_meta(
            "c1",
            json!({"toolUseId": inner}).as_object().unwrap().clone(),
        );
        let line =
            herdr_agent_kit::agent_session::claude::Line::parse(call_line.as_bytes()).unwrap();
        for owner in [first, second] {
            s.apply(Owner::Agent(owner.into()), &line, line.ts());
        }
        assert_eq!(
            s.parentage().parent_of("c1"),
            Some(Owner::Agent(first.into())),
            "{first} read first"
        );
    }
}
