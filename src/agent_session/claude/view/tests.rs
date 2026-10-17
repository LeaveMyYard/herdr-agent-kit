use super::*;
use crate::agent_session::Ts;
use crate::agent_session::claude::Line;
use crate::agent_session::claude::fold::Owner;

const OWN: &str = "00000000-0000-4000-8000-000000000001";
const NEXT: &str = "00000000-0000-4000-8000-000000000002";
const LAST: &str = "00000000-0000-4000-8000-000000000003";

fn id(s: &str) -> SessionId {
    SessionId::parse(s).unwrap()
}

fn feed(s: &mut Session, raw: &str) {
    let line = Line::parse(raw.as_bytes()).unwrap();
    let ts = line.ts();
    s.apply(Owner::Main, &line, ts);
}

#[test]
fn title_takes_the_first_non_empty_of_custom_agent_ai() {
    let mut s = Session::new(id(OWN));
    let v = s.view(Status::Run, None, vec![], &[]);
    assert_eq!(v.title, None);
    feed(&mut s, r#"{"type":"ai-title","aiTitle":"ai"}"#);
    feed(&mut s, r#"{"type":"agent-name","agentName":"agent"}"#);
    assert_eq!(
        s.view(Status::Run, None, vec![], &[]).title.as_deref(),
        Some("agent")
    );
    feed(&mut s, r#"{"type":"custom-title","customTitle":""}"#);
    assert_eq!(
        s.view(Status::Run, None, vec![], &[]).title.as_deref(),
        Some("agent")
    );
    feed(&mut s, r#"{"type":"custom-title","customTitle":"mine"}"#);
    assert_eq!(
        s.view(Status::Run, None, vec![], &[]).title.as_deref(),
        Some("mine")
    );
}

#[test]
fn successors_are_the_hops_after_the_session_with_their_times() {
    let s = Session::new(id(OWN));
    let chain = [
        Hop::new(id(OWN), true, None),
        Hop::new(id(NEXT), true, Some(Ts(10.0))),
        Hop::new(id(LAST), false, None),
    ];
    let v = s.view(Status::Done, Some(id(LAST)), vec![id(NEXT)], &chain);
    assert_eq!(v.id, id(OWN));
    assert_eq!(v.status, Status::Done);
    assert_eq!(v.parent, Some(id(LAST)));
    assert_eq!(v.children, vec![id(NEXT)]);
    assert_eq!(
        v.successors,
        vec![(id(NEXT), Some(Ts(10.0))), (id(LAST), None)]
    );
    let other = [Hop::new(id(NEXT), true, None)];
    assert!(
        s.view(Status::Done, None, vec![], &other)
            .successors
            .is_empty()
    );
}
