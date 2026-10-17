//! `agent_session::claude::io` against real files: discovery, the tail
//! summary and continuation followers (the tailer itself:
//! `agent_session_tail.rs`). Records are captured templates (`tests/common`).

mod common;

use std::fs;

use common::{BASE, CWD, Project, Transcript, dumps, stamp, tmp_dir, tpl};
use herdr_agent_kit::agent_session::claude::io::{
    Follower, ProjectsRoot, Transcripts, related, summarize_tail, summarize_tail_from,
    workflow_summary_path,
};
use herdr_agent_kit::agent_session::claude::{
    Hop, Lookup, TranscriptFiles, chain, continuation::MAX_HOPS,
};
use herdr_agent_kit::agent_session::{SessionId, Ts};
use serde_json::{Value, json};

const SID: &str = "55555555-5555-4555-8555-555555555555";
const NEXT: &str = "66666666-6666-4666-8666-666666666666";
const THIRD: &str = "77777777-7777-4777-8777-777777777777";

fn id(s: &str) -> SessionId {
    SessionId::parse(s).expect("a session id")
}

#[test]
fn a_session_is_found_in_its_cwd_project_first_then_anywhere() {
    let tmp = tmp_dir("find");
    let root = ProjectsRoot::new(tmp.join("projects"));
    assert_eq!(root.find(&id(SID), Some(CWD)), None); // no root dir at all: nothing, no error
    let proj = Project::new(root.path().to_path_buf());
    let other = root.path().join("-elsewhere");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join(format!("{SID}.jsonl")), "").unwrap();
    assert_eq!(
        root.find(&id(SID), Some(CWD)),
        Some(other.join(format!("{SID}.jsonl")))
    );
    let m = proj.main(SID);
    m.write();
    assert_eq!(root.find(&id(SID), Some(CWD)), Some(m.path.clone()));
    assert_eq!(root.project_dir(CWD), proj.dir);
}

#[test]
fn related_lists_agent_files_and_workflow_journals() {
    let tmp = tmp_dir("related");
    let proj = Project::new(tmp.join("projects"));
    let main = proj.main(SID);
    main.write();
    let none = related(&main.path);
    assert!(none.agents.is_empty() && none.journals.is_empty());
    let subs = proj.dir.join(SID).join("subagents");
    let wf = subs.join("workflows").join("run1");
    fs::create_dir_all(&wf).unwrap();
    for name in [
        "agent-b.jsonl",
        "agent-a.jsonl",
        "agent-.jsonl",
        "agent-a.meta.json",
        "other.jsonl",
    ] {
        fs::write(subs.join(name), "").unwrap();
    }
    fs::write(wf.join("agent-w1.jsonl"), "").unwrap();
    let found = related(&main.path);
    let (files, journals) = (found.agents, found.journals);
    let ids: Vec<(&str, Option<&str>)> = files
        .iter()
        .map(|f| (f.agent_id.as_str(), f.workflow.as_deref()))
        .collect();
    assert_eq!(ids, vec![("a", None), ("b", None), ("w1", Some("run1"))]);
    assert_eq!(files[0].transcript, subs.join("agent-a.jsonl"));
    assert_eq!(files[0].meta, subs.join("agent-a.meta.json"));
    assert_eq!(
        journals,
        vec![("run1".to_string(), wf.join("journal.jsonl"))]
    );
    assert_eq!(
        workflow_summary_path(&main.path, "run1"),
        proj.dir.join(SID).join("workflows").join("run1.json")
    );
}

fn tool_use(m: &mut Transcript, t: i64, template: &str, name: &str, tid: &str, input: Value) {
    let mut rec = tpl(template);
    let mut block = rec["message"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == "tool_use")
        .unwrap()
        .clone();
    block["name"] = json!(name);
    block["id"] = json!(tid);
    block["input"] = input;
    rec["message"]["content"] = json!([block]);
    m.add(rec, t);
}

fn tool_result(m: &mut Transcript, t: i64, tid: &str) {
    let mut rec = tpl("result_bash");
    let mut block = rec["message"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == "tool_result")
        .unwrap()
        .clone();
    block["tool_use_id"] = json!(tid);
    block["content"] = json!([{"type": "text", "text": "ok"}]);
    rec["message"]["content"] = json!([block]);
    m.add(rec, t);
}

#[test]
fn the_tail_summary_reads_activity_pending_calls_text_and_model() {
    let tmp = tmp_dir("summary");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.main(SID);
    m.prompt(BASE, "work");
    m.text(BASE + 1, "  thinking  ");
    tool_use(
        &mut m,
        BASE + 2,
        "assistant_bash",
        "Bash",
        "toolu_1",
        json!({"command": "ls", "description": "List files"}),
    );
    tool_result(&mut m, BASE + 3, "toolu_1");
    tool_use(
        &mut m,
        BASE + 4,
        "assistant_bash",
        "Read",
        "toolu_2",
        json!({"file_path": format!("{CWD}/src/a.rs")}),
    );
    tool_use(
        &mut m,
        BASE + 5,
        "assistant_bash",
        "Bash",
        "toolu_3",
        json!({"command": "sleep 1"}),
    );
    m.write();
    let info = summarize_tail(&m.path, Some(CWD));
    assert_eq!(info.first_ts, Some(Ts(BASE as f64)));
    assert_eq!(info.last_ts, Some(Ts((BASE + 5) as f64)));
    assert_eq!(info.model.as_deref(), Some("claude-model"));
    assert_eq!(info.last_text, "thinking");
    assert_eq!(
        info.pending
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Read", "Bash"]
    );
    assert_eq!(info.pending[0].ts, Some(Ts((BASE + 4) as f64)));
    assert_eq!(info.activity, "Bash(sleep 1)");
    assert!(!info.handback);
    // every call answered: the activity is the last call made
    tool_result(&mut m, BASE + 6, "toolu_2");
    tool_result(&mut m, BASE + 7, "toolu_3");
    m.write();
    let info = summarize_tail(&m.path, Some(CWD));
    assert!(info.pending.is_empty());
    assert_eq!(info.activity, "Bash(sleep 1)");
    assert_eq!(info.last_ts, Some(Ts((BASE + 7) as f64)));
}

#[test]
fn the_tail_summary_sees_a_handback_and_skips_placeholder_models() {
    let tmp = tmp_dir("handback");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.agent(SID, "a1");
    let mut rec = tpl("sub_assistant_text");
    rec["message"]["model"] = json!("<synthetic>");
    m.add(rec, BASE);
    tool_use(
        &mut m,
        BASE + 1,
        "sub_assistant_handback",
        "SubagentHandback",
        "toolu_9",
        json!({"message": "done"}),
    );
    m.write();
    let info = summarize_tail(&m.path, None);
    assert!(info.handback);
    assert_eq!(info.model.as_deref(), Some("claude-model"));
    assert_eq!(info.activity, "SubagentHandback");
}

#[test]
fn the_tail_window_grows_past_a_huge_last_record() {
    // Two dated records, then one record far bigger than the first window:
    // the window doubles until it holds them, so the text is still seen.
    let tmp = tmp_dir("window");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.main(SID);
    m.prompt(BASE, "work");
    m.text(BASE + 1, "the answer");
    let mut big = stamp(tpl("result_bash"), BASE + 2, SID);
    big["toolUseResult"]["stdout"] = json!("x".repeat(200_000));
    m.raw(dumps(&big));
    m.write();
    let info = summarize_tail_from(&m.path, None, 1024);
    assert_eq!(info.last_text, "the answer");
    assert_eq!(info.last_ts, Some(Ts((BASE + 2) as f64)));
    // a file that is not there: an empty summary
    assert_eq!(
        summarize_tail(&tmp.join("nope.jsonl"), None),
        Default::default()
    );
}

#[test]
fn a_head_line_without_its_newline_gives_no_first_time() {
    let tmp = tmp_dir("head");
    let p = tmp.join("h.jsonl");
    let rec = stamp(tpl("user_prompt_human"), BASE, SID);
    fs::write(&p, dumps(&rec)).unwrap();
    assert_eq!(summarize_tail(&p, None).first_ts, None);
    fs::write(&p, format!("{}\n", dumps(&rec))).unwrap();
    assert_eq!(summarize_tail(&p, None).first_ts, Some(Ts(BASE as f64)));
}

fn continued(m: &mut Transcript, t: i64, nxt: &str) {
    let mut rec = tpl("continued_in");
    rec.insert("timestamp".into(), json!(common::iso(t)));
    rec.insert("sessionId".into(), json!(m.session));
    rec.insert("continuedInSessionId".into(), json!(nxt));
    m.raw(dumps(&rec));
}

#[test]
fn a_rewritten_transcript_is_read_again_from_the_start() {
    let tmp = tmp_dir("rewrite");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.main(SID);
    m.prompt(BASE, "work");
    continued(&mut m, BASE + 10, NEXT);
    m.write();
    let mut f = Follower::new(&m.path, id(SID));
    assert!(f.update());
    assert_eq!(f.continued_in(), Some(&id(NEXT)));
    let mut again = proj.main(SID); // shorter: the continuation is gone
    again.prompt(BASE, "work");
    again.write();
    assert!(f.update());
    assert_eq!(f.continued_in(), None);
    fs::remove_file(&m.path).unwrap();
    assert!(!f.update());
}

#[test]
fn transcripts_follow_a_chain_forward_and_notice_a_next_session_appearing() {
    let tmp = tmp_dir("transcripts");
    let proj = Project::new(tmp.join("projects"));
    let mut a = proj.main(SID);
    a.prompt(BASE, "work");
    continued(&mut a, BASE + 10, NEXT);
    a.write();
    let mut files = Transcripts::new(ProjectsRoot::new(&proj.root), Some(CWD.into()));
    assert_eq!(
        chain(&id(SID), &mut files, MAX_HOPS),
        vec![
            Hop::new(id(SID), true, None),
            Hop::new(id(NEXT), false, Some(Ts((BASE + 10) as f64)))
        ]
    );
    let mut b = proj.main(NEXT);
    b.prompt(BASE + 11, "next");
    continued(&mut b, BASE + 20, THIRD);
    b.write();
    let mut c = proj.main(THIRD);
    c.prompt(BASE + 21, "third");
    c.write();
    let hops = chain(&id(SID), &mut files, MAX_HOPS);
    assert_eq!(
        hops.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
        vec![SID, NEXT, THIRD]
    );
    assert!(hops.iter().all(|h| h.found));
    // forward only: from the middle, the session that continued into it is not on the chain
    let hops = chain(&id(NEXT), &mut files, MAX_HOPS);
    assert_eq!(
        hops.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
        vec![NEXT, THIRD]
    );
    // the old session is resumed: it no longer continues anywhere
    a.prompt(BASE + 30, "back again");
    a.write();
    assert_eq!(files.lookup(&id(SID)), Lookup::Ended);
    assert_eq!(
        chain(&id(SID), &mut files, MAX_HOPS),
        vec![Hop::new(id(SID), true, None)]
    );
}

#[test]
fn a_remembered_path_is_used_while_it_exists() {
    let tmp = tmp_dir("remember");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.main(SID);
    m.prompt(BASE, "work");
    continued(&mut m, BASE + 10, NEXT);
    m.write();
    let elsewhere = tmp.join("elsewhere.jsonl");
    fs::write(&elsewhere, "").unwrap();
    let mut files = Transcripts::new(ProjectsRoot::new(&proj.root), None);
    files.remember(id(SID), elsewhere.clone());
    assert_eq!(files.find(&id(SID)), Some(elsewhere.clone()));
    assert_eq!(files.lookup(&id(SID)), Lookup::Ended);
    fs::remove_file(&elsewhere).unwrap();
    assert_eq!(files.find(&id(SID)), Some(m.path.clone()));
    assert_eq!(
        files.lookup(&id(SID)),
        Lookup::continued_in(id(NEXT), Some(Ts((BASE + 10) as f64)))
    );
}

#[test]
fn a_rewrite_forgets_the_time_read_before_it() {
    // Before the rewrite the file's last dated record is late (BASE + 100).
    // The rewritten file starts with an undated continued-in: nothing dated
    // before it, so it has no time and any conversation record clears it,
    // even one earlier than the old file's records.
    let tmp = tmp_dir("rewrite-time");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.main(SID);
    for i in 0..5 {
        m.prompt(
            BASE + 100,
            &format!("a long prompt that makes this file longer {i}"),
        );
    }
    m.write();
    let mut f = Follower::new(&m.path, id(SID));
    assert!(f.update());
    let mut again = proj.main(SID);
    let mut rec = tpl("continued_in");
    rec.remove("timestamp");
    rec.insert("sessionId".into(), json!(SID));
    rec.insert("continuedInSessionId".into(), json!(NEXT));
    again.raw(dumps(&rec));
    again.write();
    assert!(f.update());
    assert_eq!(f.continued_in(), Some(&id(NEXT)));
    again.prompt(BASE + 50, "go on");
    again.write();
    assert!(f.update());
    assert_eq!(f.continued_in(), None);
}

#[test]
fn a_large_transcript_is_followed_in_bounded_steps() {
    // a follower reads a file bigger than one step (`Tailer::read_at_most`) to its end
    let tmp = tmp_dir("chunks");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.main(SID);
    let filler = "x".repeat(1024 * 1024);
    for i in 0..10 {
        m.prompt(BASE + i, &filler);
    }
    continued(&mut m, BASE + 20, NEXT);
    m.write();
    assert!(
        fs::metadata(&m.path).unwrap().len()
            > herdr_agent_kit::agent_session::claude::io::FOLLOW_CHUNK
    );
    let mut f = Follower::new(&m.path, id(SID));
    assert!(f.update());
    assert_eq!(f.continued_in(), Some(&id(NEXT)));
}

#[test]
fn a_zero_window_still_summarizes() {
    let tmp = tmp_dir("zero-window");
    let p = tmp.join("z.jsonl");
    fs::write(
        &p,
        format!("{}\n", dumps(&stamp(tpl("user_prompt_human"), BASE, SID))),
    )
    .unwrap();
    assert_eq!(
        summarize_tail_from(&p, None, 0).last_ts,
        Some(Ts(BASE as f64))
    );
}

#[test]
fn a_remembered_path_is_preferred_over_one_found_before() {
    let tmp = tmp_dir("remember-late");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.main(SID);
    m.prompt(BASE, "work");
    m.write();
    let mut files = Transcripts::new(ProjectsRoot::new(&proj.root), None);
    assert_eq!(files.lookup(&id(SID)), Lookup::Ended);
    let elsewhere = tmp.join("elsewhere.jsonl");
    let mut rec = tpl("continued_in");
    rec.insert("sessionId".into(), json!(SID));
    rec.insert("continuedInSessionId".into(), json!(NEXT));
    fs::write(&elsewhere, format!("{}\n", dumps(&rec))).unwrap();
    files.remember(id(SID), elsewhere);
    assert!(matches!(
        files.lookup(&id(SID)),
        Lookup::ContinuedIn { next, .. } if next == id(NEXT)
    ));
}

#[cfg(unix)]
#[test]
fn a_transcript_that_cannot_be_read_is_not_found() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tmp_dir("unreadable");
    let proj = Project::new(tmp.join("projects"));
    let mut m = proj.main(SID);
    m.prompt(BASE, "work");
    m.write();
    fs::set_permissions(&m.path, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::File::open(&m.path).is_ok() {
        // root reads any file: nothing to check here (CI runs unprivileged)
        fs::set_permissions(&m.path, fs::Permissions::from_mode(0o644)).unwrap();
        return;
    }
    // the file is there (found by search) but reading it fails: not found, never Ended
    let mut files = Transcripts::new(ProjectsRoot::new(&proj.root), None);
    assert_eq!(files.find(&id(SID)), Some(m.path.clone()));
    assert_eq!(files.lookup(&id(SID)), Lookup::NotFound);
    assert_eq!(
        chain(&id(SID), &mut files, MAX_HOPS),
        vec![Hop::new(id(SID), false, None)]
    );
    fs::set_permissions(&m.path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(files.lookup(&id(SID)), Lookup::Ended);
}
