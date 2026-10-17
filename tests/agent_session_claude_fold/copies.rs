//! Which file's copy counts when one record is read from several files (a
//! fork's file repeats its parent's history): main's, then the earliest, in
//! either read order. Spawning calls ([`Parentage::spawn_of`] and
//! [`Parentage::parent_of`]) and background launches
//! ([`Background::open`]).
//!
//! panorama has no test for this. Each case was run against panorama `main`'s
//! `scan.session.Session.apply` (both orders) with the same records, and gave
//! the same answers.
//!
//! [`Parentage::spawn_of`]: herdr_agent_kit::agent_session::claude::fold::Parentage::spawn_of
//! [`Parentage::parent_of`]: herdr_agent_kit::agent_session::claude::fold::Parentage::parent_of
//! [`Background::open`]: herdr_agent_kit::agent_session::claude::fold::Background::open

use super::*;
use herdr_agent_kit::agent_session::claude::{Line, Session};

/// One file: whose it is, its lines, and whether they are folded at their
/// own times (`false`: undated, folded without a time).
struct File {
    owner: Owner,
    lines: Vec<String>,
    dated: bool,
}

fn tx(agent: Option<&str>) -> Tx {
    Tx {
        path: PathBuf::new(),
        agent: agent.map(str::to_string),
        lines: Vec::new(),
        n: 0,
    }
}

fn owner(agent: Option<&str>) -> Owner {
    agent.map_or(Owner::Main, |a| Owner::Agent(a.into()))
}

/// The spawning call `call` of agent `a1`, as `agent`'s file (main for
/// `None`) holds it at `t`.
fn spawn_copy(agent: Option<&str>, call: &str, t: f64, dated: bool) -> File {
    let mut tx = tx(agent);
    let rec = tx.kind("assistant_agent", "sub_assistant_agent");
    let input = json!({"description": "x", "subagent_type": "general-purpose", "prompt": "brief"});
    tx.add(tool_block(rec, "Agent", call, input), t);
    File {
        owner: owner(agent),
        lines: tx.lines,
        dated,
    }
}

/// Background task `b1`'s launch (its Bash call and result), as `agent`'s
/// file holds it at `t`.
fn bg_copy(agent: Option<&str>, t: f64, dated: bool) -> File {
    let mut tx = tx(agent);
    tx.bash_background(t, "b1");
    File {
        owner: owner(agent),
        lines: tx.lines,
        dated,
    }
}

/// `files` folded in this order into a session where agent `a1` was
/// spawned by call `toolu_x`.
fn fold(files: &[&File]) -> Session {
    let mut s = Session::new(SessionId::parse(SID).unwrap());
    let meta = json!({"toolUseId": "toolu_x"});
    s.set_meta("a1", meta.as_object().unwrap().clone());
    for f in files {
        for raw in &f.lines {
            let line = Line::parse(raw.as_bytes()).unwrap();
            let when = if f.dated { line.ts() } else { None };
            s.apply(f.owner.clone(), &line, when);
        }
    }
    s
}

/// Both read orders of two files.
fn both_orders(a: &File, b: &File) -> [Session; 2] {
    [fold(&[a, b]), fold(&[b, a])]
}

/// `a1`'s spawning-call owner and parent, in both read orders.
fn spawn_answers(a: &File, b: &File) -> Vec<(Option<Owner>, Option<Owner>)> {
    both_orders(a, b)
        .iter()
        .map(|s| {
            let p = s.parentage();
            (
                p.spawn_of("a1").map(|sp| sp.owner.clone()),
                p.parent_of("a1"),
            )
        })
        .collect()
}

/// The owners `b1` is open under, of `candidates`, in both read orders.
fn bg_owners(a: &File, b: &File, candidates: &[Owner]) -> Vec<Vec<Owner>> {
    both_orders(a, b)
        .iter()
        .map(|s| {
            candidates
                .iter()
                .filter(|o| {
                    s.background()
                        .open(o, Ts(B + 30.0 * M))
                        .any(|l| l.task == "b1")
                })
                .cloned()
                .collect()
        })
        .collect()
}

fn agent(id: &str) -> Owner {
    Owner::Agent(id.into())
}

#[test]
fn main_s_copy_of_a_spawning_call_wins_even_when_it_is_later() {
    let main = spawn_copy(None, "toolu_x", B + 20.0, true);
    let f1 = spawn_copy(Some("f1"), "toolu_x", B + 10.0, true);
    let want = (Some(Owner::Main), Some(Owner::Main));
    assert_eq!(spawn_answers(&main, &f1), [want.clone(), want]);
}

#[test]
fn of_two_agent_copies_of_a_spawning_call_the_earliest_wins() {
    let f1 = spawn_copy(Some("f1"), "toolu_x", B + 20.0, true);
    let f2 = spawn_copy(Some("f2"), "toolu_x", B + 10.0, true);
    let want = (Some(agent("f2")), Some(agent("f2")));
    assert_eq!(spawn_answers(&f1, &f2), [want.clone(), want]);
}

#[test]
fn an_undated_copy_of_a_spawning_call_sorts_last() {
    let f1 = spawn_copy(Some("f1"), "toolu_x", B + 10.0, true);
    let f2 = spawn_copy(Some("f2"), "toolu_x", B + 5.0, false);
    let want = (Some(agent("f1")), Some(agent("f1")));
    assert_eq!(spawn_answers(&f1, &f2), [want.clone(), want]);
}

/// panorama parity: a copy at time `0` sorts last for the kept spawn
/// (`(when or inf)`) but first for the parent (`min` over the raw times).
#[test]
fn a_copy_at_time_zero_sorts_last_for_the_spawn_and_first_for_the_parent() {
    let f1 = spawn_copy(Some("f1"), "toolu_x", 0.0, true);
    let f2 = spawn_copy(Some("f2"), "toolu_x", B + 10.0, true);
    let want = (Some(agent("f2")), Some(agent("f1")));
    assert_eq!(spawn_answers(&f1, &f2), [want.clone(), want]);
}

#[test]
fn main_s_copy_of_a_background_launch_wins_even_when_it_is_later() {
    let main = bg_copy(None, B + 20.0, true);
    let f1 = bg_copy(Some("f1"), B + 10.0, true);
    let owners = bg_owners(&main, &f1, &[Owner::Main, agent("f1")]);
    assert_eq!(owners, [vec![Owner::Main], vec![Owner::Main]]);
}

#[test]
fn of_two_agent_copies_of_a_background_launch_the_earliest_wins() {
    let f1 = bg_copy(Some("f1"), B + 20.0, true);
    let f2 = bg_copy(Some("f2"), B + 10.0, true);
    let owners = bg_owners(&f1, &f2, &[agent("f1"), agent("f2")]);
    assert_eq!(owners, [vec![agent("f2")], vec![agent("f2")]]);
}

#[test]
fn an_undated_copy_of_a_background_launch_sorts_last() {
    let f1 = bg_copy(Some("f1"), B + 10.0, true);
    let f2 = bg_copy(Some("f2"), B + 5.0, false);
    let owners = bg_owners(&f1, &f2, &[agent("f1"), agent("f2")]);
    assert_eq!(owners, [vec![agent("f1")], vec![agent("f1")]]);
}
