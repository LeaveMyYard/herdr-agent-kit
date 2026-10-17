use super::*;
use std::collections::HashMap;

fn sid(n: u8) -> SessionId {
    SessionId::parse(&format!("{n:08x}-0000-4000-8000-000000000000")).unwrap()
}

struct Map(HashMap<SessionId, Option<SessionId>>);

impl TranscriptFiles for Map {
    fn continued_in(&mut self, id: &SessionId) -> Option<Option<SessionId>> {
        self.0.get(id).cloned()
    }
}

fn feed(c: &mut Continuation, lines: &[&str]) {
    c.feed(&lines.iter().map(|l| l.as_bytes()).collect::<Vec<_>>());
}

#[test]
fn a_time_of_zero_is_no_time_for_the_continuation() {
    // both Pythons: `ts or last`, and the inbox's `if when: last = when`
    let own = sid(1);
    let to = sid(2);
    let cont = |ts: &str| {
        format!(r#"{{"type":"continued-in","timestamp":{ts},"continuedInSessionId":"{to}"}}"#)
    };
    // dated 0: takes the last dated record's time (10), so a resume at 5 does not clear it
    let mut c = Continuation::new(own.clone());
    feed(
        &mut c,
        &[
            r#"{"type":"user","timestamp":10}"#,
            &cont("0"),
            r#"{"type":"user","timestamp":5}"#,
        ],
    );
    assert_eq!(c.next(), Some(&to));
    assert_eq!(c.at(), Some(Ts(10.0)));
    // a record dated 0 does not date an undated continuation
    let mut c = Continuation::new(own.clone());
    feed(
        &mut c,
        &[
            r#"{"type":"user","timestamp":10}"#,
            r#"{"type":"mode","timestamp":"0"}"#,
            &cont("null"),
        ],
    );
    assert_eq!(c.at(), Some(Ts(10.0)));
    // a conversation record dated 0 is still dated: not later than 10
    feed(&mut c, &[r#"{"type":"user","timestamp":0}"#]);
    assert_eq!(c.next(), Some(&to));
}

#[test]
fn a_resume_holding_a_lone_surrogate_still_clears_it() {
    // Node writes a lone surrogate escape when it cuts a string inside an emoji
    let mut c = Continuation::new(sid(1));
    let to = sid(2);
    feed(
        &mut c,
        &[
            &format!(r#"{{"type":"continued-in","timestamp":10,"continuedInSessionId":"{to}"}}"#),
            r#"{"type":"user","timestamp":20,"message":{"role":"user","content":"cut \ud83d"}}"#,
        ],
    );
    assert_eq!(c.next(), None);
}

#[test]
fn chain_follows_forward_and_marks_an_unwritten_target() {
    let mut files = Map(HashMap::from([
        (sid(1), Some(sid(2))),
        (sid(2), Some(sid(3))),
    ]));
    let hops = chain(&sid(1), &mut files, MAX_HOPS);
    assert_eq!(
        hops,
        vec![
            Hop::new(sid(1), true, None),
            Hop::new(sid(2), true, None),
            Hop::new(sid(3), false, None)
        ]
    );
    // forward only: from the middle, the ancestor is not part of the chain
    let hops = chain(&sid(2), &mut files, MAX_HOPS);
    assert_eq!(
        hops.iter().map(|h| h.id.clone()).collect::<Vec<_>>(),
        vec![sid(2), sid(3)]
    );
}

#[test]
fn chain_stops_at_a_loop_and_after_max_hops() {
    let mut files = Map(HashMap::from([
        (sid(1), Some(sid(2))),
        (sid(2), Some(sid(1))),
    ]));
    assert_eq!(chain(&sid(1), &mut files, MAX_HOPS).len(), 2);
    let mut long = Map((1..=9).map(|n| (sid(n), Some(sid(n + 1)))).collect());
    let hops = chain(&sid(1), &mut long, 3);
    assert_eq!(hops.len(), 4);
    assert!(hops.iter().all(|h| h.found));
    let hops = chain(&sid(1), &mut long, MAX_HOPS);
    assert_eq!(hops.len(), MAX_HOPS + 1);
    let mut none = Map(HashMap::new());
    assert_eq!(
        chain(&sid(1), &mut none, MAX_HOPS),
        vec![Hop::new(sid(1), false, None)]
    );
}
