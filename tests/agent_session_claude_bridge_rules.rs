//! The rules both plugins share (plan `docs/plans/rust-rewrite.md` §1.3 rows
//! 3-4, D6), ported case for case from the bridged rules table:
//! herdr-panorama `tests/test_bridge_rules.py` (main `4f525f3`, #53) and the
//! rows herdr-inbox's copy adds (main `e209939`, #47).
//!
//! - one session-id validator ([`SessionId`]) and its canonical form;
//! - the continuation rule (D6, [`Continuation`]): a `continued-in` record of
//!   the main transcript (not a sidechain) naming another session id
//!   (compared in canonical lowercase) is cleared only by a later `user` or
//!   `assistant` record, never by bookkeeping records, known or new.
//!
//! Python rows with no counterpart in this module (panorama's `machines` and
//! `events` modules, and `is_claude`, which belongs to `agent_procs`) are
//! listed in the PR that ported this file.
//!
//! Every transcript record is a captured template (`tests/common`).

mod common;

use common::{BASE, Project, Transcript, dumps, iso, stamp, tmp_dir, tpl};
use herdr_agent_kit::agent_session::claude::io::{Follower, ProjectsRoot, Transcripts};
use herdr_agent_kit::agent_session::claude::{Hop, chain, continuation::MAX_HOPS};
use herdr_agent_kit::agent_session::{SessionId, Ts};
use serde_json::{Value, json};

const SID: &str = "55555555-5555-4555-8555-555555555555";
const NEXT: &str = "66666666-6666-4666-8666-666666666666";
/// Has letters: `HEX.to_uppercase() != HEX`.
const HEX: &str = "abcdef01-2345-4abc-8def-0123456789ab";

fn id(s: &str) -> SessionId {
    SessionId::parse(s).expect("a session id")
}

/// Python's `str.title()` of a UUID: each letter after a non-letter is
/// uppercased, every other letter lowercased.
fn title(s: &str) -> String {
    let mut out = String::new();
    let mut prev_letter = false;
    for c in s.chars() {
        let letter = c.is_alphabetic();
        out.push(if letter && !prev_letter {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        });
        prev_letter = letter;
    }
    out
}

// -- one session-id validator ---------------------------------------------------------------

const VALID: [(&str, &str); 4] = [
    (SID, SID),
    (
        "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
        "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
    ),
    // any case, canonical lower
    (
        "0A1B2C3D-4E5F-4A6B-8C7D-9E0F1A2B3C4D",
        "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
    ),
    (
        "0a1B2c3D-4e5F-4a6B-8c7D-9e0F1a2B3c4D",
        "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
    ),
];

fn invalid() -> Vec<Value> {
    let strings: Vec<String> = vec![
        "".into(),
        format!(" {SID}"),
        format!("{SID} "),
        format!("{SID}\n"), // the whole string, no trailing newline
        format!("\t{SID}"),
        "5555555-5555-4555-8555-5555555555555".into(), // 7-4-4-4-13
        "55555555-5555-4555-8555-55555555555".into(),  // short
        "55555555-5555-4555-8555-5555555555555".into(), // long
        "555555555555455585555555555555555".into(),    // no dashes
        "55555555_5555_4555_8555_555555555555".into(),
        "g5555555-5555-4555-8555-555555555555".into(), // not hex
        format!("../{SID}"),
        format!("{SID}/.."),
        "..".into(),
        "../../outside".into(),
        "/etc/passwd".into(),
        "a/b".into(),
        "a\\b".into(),
        "55555555-5555-4555-8555-55555555555/".into(),
        "55555555-5555-4555-8555-55555555555\\".into(),
        "*".into(),
        "5555555*-5555-4555-8555-555555555555".into(),
        "55555555-5555-4555-8555-55555555555?".into(),
        "[5]555555-5555-4555-8555-555555555555".into(),
        "agent-a1".into(),
    ];
    let mut out: Vec<Value> = strings.into_iter().map(Value::String).collect();
    out.push(Value::Null); // None
    out.push(json!(123));
    out
}

#[test]
fn a_session_id_in_any_case_is_valid_and_canonical_is_lowercase() {
    for (sid, canonical) in VALID {
        assert_eq!(
            SessionId::parse(sid).as_ref().map(SessionId::as_str),
            Some(canonical),
            "{sid}"
        );
        assert_eq!(
            SessionId::from_value(&json!(sid))
                .as_ref()
                .map(SessionId::as_str),
            Some(canonical),
            "{sid}"
        );
        assert_eq!(
            sid.parse::<SessionId>().map(|s| s.to_string()),
            Ok(canonical.to_string())
        );
    }
}

#[test]
fn anything_else_is_not_a_session_id() {
    for v in invalid() {
        assert_eq!(SessionId::from_value(&v), None, "{v:?}");
        if let Some(s) = v.as_str() {
            assert_eq!(SessionId::parse(s), None, "{s:?}");
            assert!(s.parse::<SessionId>().is_err(), "{s:?}");
        }
    }
}

#[test]
fn an_uppercase_id_finds_its_lowercase_transcript() {
    // Claude Code names transcripts by the lowercase id; an id handed over in
    // another case is the same session.
    assert_ne!(HEX.to_uppercase(), HEX);
    for sid in [SID, HEX] {
        let tmp = tmp_dir("uppercase-find");
        let proj = Project::new(tmp.join("projects"));
        let mut m = proj.main(sid);
        m.text(BASE, "hi");
        m.write();
        let root = ProjectsRoot::new(&proj.root);
        assert_eq!(
            root.find(&id(&sid.to_uppercase()), None),
            Some(m.path.clone()),
            "{sid}"
        );
    }
}

#[test]
fn a_continuation_named_in_uppercase_is_found_and_followed() {
    let tmp = tmp_dir("uppercase-follow");
    let proj = Project::new(tmp.join("projects"));
    let mut old = old_session(&proj);
    continued(&mut old, BASE + 10, &HEX.to_uppercase());
    old.write();
    let mut new = proj.main(HEX);
    new.prompt(BASE + 11, "next");
    new.write();
    let mut files = Transcripts::new(ProjectsRoot::new(&proj.root), None);
    let hops = chain(&id(SID), &mut files, MAX_HOPS);
    assert_eq!(
        hops,
        vec![
            Hop::new(id(SID), true, None),
            Hop::new(id(HEX), true, Some(Ts((BASE + 10) as f64)))
        ]
    );
    assert_eq!(files.find(&hops[1].id), Some(new.path.clone()));
    // herdr naming the new session in uppercase
    assert_eq!(files.find(&id(&HEX.to_uppercase())), Some(new.path.clone()));
    let hops = chain(&id(&HEX.to_uppercase()), &mut files, MAX_HOPS);
    assert_eq!(hops, vec![Hop::new(id(HEX), true, None)]);
}

// -- the continuation rule (D6) -------------------------------------------------------------

/// A record type Claude Code may add tomorrow.
const UNKNOWN: &str = "made-up-bookkeeping";

/// A captured bookkeeping record, as Claude Code writes it (undated);
/// `UNKNOWN`: the captured `mode` record under a type this code has never
/// seen.
fn bookkeeping(m: &mut Transcript, kind: &str) {
    let mut rec = tpl(if kind == UNKNOWN { "mode" } else { kind });
    if kind == UNKNOWN {
        rec.insert("type".into(), json!(UNKNOWN));
    }
    rec.insert("sessionId".into(), json!(m.session));
    m.raw(dumps(&rec));
}

fn continued(m: &mut Transcript, t: i64, nxt: &str) {
    let mut rec = tpl("continued_in");
    rec.insert("timestamp".into(), json!(iso(t)));
    rec.insert("sessionId".into(), json!(m.session));
    rec.insert("continuedInSessionId".into(), json!(nxt));
    m.raw(dumps(&rec));
}

/// A continued-in record without a timestamp: it takes the time of the
/// record before it.
fn undated_continued(m: &mut Transcript, nxt: &str) {
    let mut rec = tpl("continued_in");
    rec.remove("timestamp");
    rec.insert("sessionId".into(), json!(m.session));
    rec.insert("continuedInSessionId".into(), json!(nxt));
    m.raw(dumps(&rec));
}

/// panorama's `Session(m.session, m.path).update()`, as far as these tables
/// look at it.
fn load(m: &Transcript) -> Follower {
    let mut f = Follower::new(&m.path, id(&m.session));
    assert!(f.update(), "the transcript exists");
    f
}

fn next(f: &Follower) -> Option<String> {
    f.continued_in().map(|s| s.as_str().to_string())
}

fn old_session(proj: &Project) -> Transcript {
    let mut m = proj.main(SID);
    m.prompt(BASE, "work");
    m.text(BASE + 1, "working");
    m
}

fn old(name: &str) -> Transcript {
    old_session(&Project::new(tmp_dir(name).join("projects")))
}

/// A user prompt or an assistant text, by kind.
fn conversation(m: &mut Transcript, kind: &str, t: i64, text: &str) {
    if kind == "user" {
        m.prompt(t, text)
    } else {
        m.text(t, text)
    }
}

#[test]
fn bookkeeping_after_continued_in_does_not_clear_it() {
    let cases: [&[&str]; 6] = [
        &[],
        &["last_prompt"],
        &["cost_state"],
        &["mode"],
        &[UNKNOWN],
        &["last_prompt", "cost_state", "mode", UNKNOWN],
    ];
    for kinds in cases {
        let mut m = old("bookkeeping");
        continued(&mut m, BASE + 10, NEXT);
        for kind in kinds {
            bookkeeping(&mut m, kind);
        }
        m.write();
        assert_eq!(next(&load(&m)).as_deref(), Some(NEXT), "{kinds:?}");
    }
}

#[test]
fn only_a_later_conversation_record_clears_it() {
    // `claude --resume <old>`: the conversation goes on in the old file, so it
    // is live again. A conversation record at or before the continued-in's
    // time is not a resume.
    for kind in ["user", "assistant"] {
        for (dt, cleared) in [(1, true), (0, false), (-1, false)] {
            let mut m = old("later-clears");
            continued(&mut m, BASE + 10, NEXT);
            bookkeeping(&mut m, "last_prompt");
            bookkeeping(&mut m, "cost_state");
            conversation(&mut m, kind, BASE + 10 + dt, "go on");
            m.write();
            let s = load(&m);
            assert_eq!(s.continued_in().is_none(), cleared, "{kind} {dt}");
            assert!(matches!(next(&s).as_deref(), None | Some(NEXT)));
        }
    }
}

#[test]
fn a_resume_appended_later_clears_it_on_the_next_read() {
    let mut m = old("resume-later");
    continued(&mut m, BASE + 10, NEXT);
    bookkeeping(&mut m, "mode");
    bookkeeping(&mut m, UNKNOWN);
    m.write();
    let mut s = load(&m);
    assert_eq!(next(&s).as_deref(), Some(NEXT));
    m.prompt(BASE + 20, "go on");
    m.write();
    assert!(s.update());
    assert_eq!(next(&s), None);
}

#[test]
fn a_continuation_that_is_not_another_session_id_is_ignored() {
    // Not a UUID, or this session's own id (in any case): never followed.
    let nxts = [
        "../../outside".to_string(),
        "*".into(),
        "/etc/passwd".into(),
        "".into(),
        SID.into(),
        SID.to_uppercase(),
        format!("{NEXT}\n"),
    ];
    for nxt in nxts {
        let mut m = old("not-a-session");
        continued(&mut m, BASE + 10, &nxt);
        m.write();
        assert_eq!(next(&load(&m)), None, "{nxt:?}");
    }
}

#[test]
fn a_continuation_in_another_case_is_kept_canonical() {
    let mut m = old("canonical");
    continued(&mut m, BASE + 10, &HEX.to_uppercase());
    m.write();
    assert_eq!(next(&load(&m)).as_deref(), Some(HEX));
}

#[test]
fn a_continuation_to_its_own_id_in_another_case_is_ignored() {
    let cases = [
        (HEX.to_string(), HEX.to_uppercase()),
        (HEX.to_uppercase(), HEX.to_string()),
        (HEX.to_string(), title(HEX)),
    ];
    assert_eq!(title(HEX), "Abcdef01-2345-4Abc-8Def-0123456789Ab");
    for (own, nxt) in cases {
        let proj = Project::new(tmp_dir("own-id").join("projects"));
        let mut m = proj.main(&own);
        m.prompt(BASE, "work");
        continued(&mut m, BASE + 10, &nxt);
        m.write();
        assert_eq!(next(&load(&m)), None, "{own} {nxt}");
    }
}

#[test]
fn an_undated_conversation_record_does_not_clear_a_dated_continuation() {
    // A resume must be LATER than the continued-in: a record with no time is not.
    for kind in ["user", "assistant"] {
        let mut m = old("undated-conversation");
        continued(&mut m, BASE + 10, NEXT);
        let mut rec = stamp(
            tpl(if kind == "user" {
                "user_prompt_human"
            } else {
                "assistant_text"
            }),
            BASE + 20,
            SID,
        );
        rec.remove("timestamp");
        m.raw(dumps(&rec));
        m.write();
        assert_eq!(next(&load(&m)).as_deref(), Some(NEXT), "{kind}");
    }
}

#[test]
fn a_sidechain_continuation_is_ignored() {
    // A continued-in record marked `isSidechain: true` is never the main
    // conversation's, even in the main transcript.
    let mut m = old("sidechain");
    let mut rec = tpl("continued_in");
    rec.insert("timestamp".into(), json!(iso(BASE + 10)));
    rec.insert("sessionId".into(), json!(SID));
    rec.insert("isSidechain".into(), json!(true));
    rec.insert("continuedInSessionId".into(), json!(NEXT));
    m.raw(dumps(&rec));
    m.write();
    assert_eq!(next(&load(&m)), None);
}

#[test]
fn a_resume_before_an_ignored_continuation_in_the_same_batch_still_counts() {
    // continued-in, then the resume, then a continued-in that is ignored, all
    // in one read: the resume scan starts after the last ACCEPTED
    // continued-in, so the resume still clears it.
    for ignored in ["sidechain", "own_id", "not_a_uuid"] {
        let mut m = old("ignored-same-batch");
        continued(&mut m, BASE + 10, NEXT);
        m.prompt(BASE + 20, "go on");
        let mut rec = tpl("continued_in");
        rec.insert("timestamp".into(), json!(iso(BASE + 30)));
        rec.insert("sessionId".into(), json!(SID));
        let target = match ignored {
            "own_id" => SID.to_uppercase(),
            "not_a_uuid" => "../x".into(),
            _ => NEXT.into(),
        };
        rec.insert("continuedInSessionId".into(), json!(target));
        if ignored == "sidechain" {
            rec.insert("isSidechain".into(), json!(true));
        }
        m.raw(dumps(&rec));
        m.write();
        assert_eq!(next(&load(&m)), None, "{ignored}");
    }
}

#[test]
fn an_undated_continuation_takes_the_previous_dated_records_time() {
    // Its time is that of the last DATED record of any type before it (here
    // the assistant text at BASE + 5, a record the scanner's prefilter does
    // not parse); undated bookkeeping between them does not move it.
    for kind in ["user", "assistant"] {
        for (dt, cleared) in [(1, true), (0, false), (-1, false)] {
            let proj = Project::new(tmp_dir("undated-previous").join("projects"));
            let mut m = proj.main(SID);
            m.prompt(BASE, "work");
            m.prompt(BASE + 1, "more");
            m.text(BASE + 5, "working");
            bookkeeping(&mut m, "mode");
            undated_continued(&mut m, NEXT);
            bookkeeping(&mut m, "cost_state");
            conversation(&mut m, kind, BASE + 5 + dt, "go on");
            m.write();
            assert_eq!(load(&m).continued_in().is_none(), cleared, "{kind} {dt}");
        }
    }
}

// -- rows herdr-inbox's copy of the table adds ----------------------------------------------

#[test]
fn an_undated_continuation_takes_a_dated_record_from_an_earlier_read() {
    // The dated record was read in an earlier batch than the undated continued-in.
    for (dt, cleared) in [(1, true), (0, false)] {
        let proj = Project::new(tmp_dir("undated-earlier-read").join("projects"));
        let mut m = proj.main(SID);
        m.prompt(BASE, "work");
        m.text(BASE + 5, "working");
        m.write();
        let mut s = load(&m);
        undated_continued(&mut m, NEXT);
        m.write();
        assert!(s.update());
        assert_eq!(next(&s).as_deref(), Some(NEXT), "{dt}");
        m.text(BASE + 5 + dt, "go on");
        m.write();
        assert!(s.update());
        assert_eq!(s.continued_in().is_none(), cleared, "{dt}");
    }
}

#[test]
fn a_nested_timestamp_does_not_date_a_record() {
    // A captured file-history-snapshot: undated, with `snapshot.timestamp`
    // inside. The undated continued-in after it takes the prompt's time
    // (BASE + 5), not the snapshot's.
    for (dt, cleared) in [(1, true), (0, false)] {
        let proj = Project::new(tmp_dir("nested-timestamp").join("projects"));
        let mut m = proj.main(SID);
        m.prompt(BASE + 5, "work");
        let mut snap = tpl("file_history_snapshot");
        snap["snapshot"]["timestamp"] = json!(iso(BASE + 100));
        m.raw(dumps(&snap));
        undated_continued(&mut m, NEXT);
        m.prompt(BASE + 5 + dt, "go on");
        m.write();
        assert_eq!(load(&m).continued_in().is_none(), cleared, "{dt}");
    }
}

#[test]
fn a_record_with_two_timestamps_is_dated_by_its_own() {
    // A captured queued_command attachment carries its own timestamp and one
    // inside `attachment`; the record's own (BASE + 5) is the one that counts.
    for (dt, cleared) in [(1, true), (0, false)] {
        let proj = Project::new(tmp_dir("two-timestamps").join("projects"));
        let mut m = proj.main(SID);
        m.prompt(BASE, "work");
        let mut att = stamp(tpl("attachment_queued_command"), BASE + 5, SID);
        att["attachment"]["timestamp"] = json!(iso(BASE + 100));
        m.raw(dumps(&att));
        undated_continued(&mut m, NEXT);
        m.prompt(BASE + 5 + dt, "go on");
        m.write();
        assert_eq!(load(&m).continued_in().is_none(), cleared, "{dt}");
    }
}

#[test]
fn an_undated_continuation_with_no_time_before_it_is_cleared_by_any_conversation() {
    // Nothing dated before it: no time to compare with, so any later
    // user/assistant record is a resume; bookkeeping still is not.
    let proj = Project::new(tmp_dir("undated-no-time").join("projects"));
    let mut m = proj.main(SID);
    undated_continued(&mut m, NEXT);
    bookkeeping(&mut m, "last_prompt");
    bookkeeping(&mut m, UNKNOWN);
    m.write();
    let mut s = load(&m);
    assert_eq!(next(&s).as_deref(), Some(NEXT));
    m.text(BASE, "go on");
    m.write();
    assert!(s.update());
    assert_eq!(next(&s), None);
}

#[test]
fn a_continuation_from_a_subagent_file_is_ignored() {
    // Only the main transcript's continued-in counts.
    let proj = Project::new(tmp_dir("subagent-file").join("projects"));
    let mut agent = proj.agent(SID, "a1");
    let mut main = proj.main(SID);
    for t in [&mut agent, &mut main] {
        t.prompt(BASE, "work");
        continued(t, BASE + 10, NEXT);
        t.write();
    }
    assert_eq!(next(&load(&agent)), None);
    assert_eq!(next(&load(&main)).as_deref(), Some(NEXT));
}
