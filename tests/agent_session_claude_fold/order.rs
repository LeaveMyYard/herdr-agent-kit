//! The fold does not depend on the order files are read.

use super::*;

#[test]
fn the_fold_does_not_depend_on_the_order_files_are_read() {
    // the same records applied in two file orders give the same statuses
    let mut sc = Sc::new("order");
    let call = bg_agent(&mut sc, "a1", "x");
    sc.agents
        .get_mut("a1")
        .unwrap()
        .0
        .bash(B + M, "work", false);
    sc.main.notify(
        B + 2.0 * M,
        "a1",
        &call,
        "completed",
        &["queue", "attachment"],
    );
    let s = sc.load();
    let lines_main = std::fs::read(&sc.main.path).unwrap();
    let lines_a1 = std::fs::read(&sc.agents["a1"].0.path).unwrap();
    let split = |b: &[u8]| -> Vec<Vec<u8>> {
        b.split(|c| *c == b'\n')
            .filter(|l| !l.is_empty())
            .map(<[u8]>::to_vec)
            .collect()
    };
    let mut pure =
        herdr_agent_kit::agent_session::claude::Session::new(SessionId::parse(SID).unwrap());
    pure.set_agent("a1", sc.agents["a1"].0.path.clone(), None);
    pure.set_meta("a1", sc.agents["a1"].1.clone());
    for (owner, lines) in [
        (Owner::Agent("a1".into()), split(&lines_a1)),
        (Owner::Main, split(&lines_main)),
    ] {
        for raw in &lines {
            let line = herdr_agent_kit::agent_session::claude::Line::parse(raw).unwrap();
            let when = line.ts();
            let facts = pure.file_mut(owner.clone());
            if let Some(t) = when {
                facts.last_ts = Some(t);
            }
            pure.apply(owner.clone(), &line, when);
        }
    }
    let want = s.session().status_of("a1", Ts(B + 30.0 * M), &NoProcs);
    let got = pure.status_of("a1", Ts(B + 30.0 * M), &NoProcs);
    assert_eq!(
        (got.status, got.ended, got.inferred),
        (want.status, want.ended, want.inferred)
    );
}
