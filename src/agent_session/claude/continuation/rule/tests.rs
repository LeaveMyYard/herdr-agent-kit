use super::*;

fn sid(n: u8) -> SessionId {
    SessionId::parse(&format!("{n:08x}-0000-4000-8000-000000000000")).unwrap()
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
