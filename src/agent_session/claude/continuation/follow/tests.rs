use super::*;
use std::collections::HashMap;

fn sid(n: u8) -> SessionId {
    SessionId::parse(&format!("{n:08x}-0000-4000-8000-000000000000")).unwrap()
}

/// A source without times: `None` is a session that did not continue.
struct Map(HashMap<SessionId, Option<SessionId>>);

impl TranscriptFiles for Map {
    fn lookup(&mut self, id: &SessionId) -> Lookup {
        match self.0.get(id) {
            None => Lookup::NotFound,
            Some(None) => Lookup::Ended,
            Some(Some(next)) => Lookup::ContinuedIn {
                next: next.clone(),
                at: None,
            },
        }
    }
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

#[test]
fn each_hop_carries_the_time_of_the_continuation_that_led_to_it() {
    struct Timed;
    impl TranscriptFiles for Timed {
        fn lookup(&mut self, id: &SessionId) -> Lookup {
            match id {
                id if *id == sid(1) => Lookup::ContinuedIn {
                    next: sid(2),
                    at: Some(Ts(10.0)),
                },
                id if *id == sid(2) => Lookup::ContinuedIn {
                    next: sid(3),
                    at: None,
                },
                _ => Lookup::Ended,
            }
        }
    }
    assert_eq!(
        chain(&sid(1), &mut Timed, MAX_HOPS),
        vec![
            Hop::new(sid(1), true, None),
            Hop::new(sid(2), true, Some(Ts(10.0))),
            Hop::new(sid(3), true, None)
        ]
    );
}
