//! Updates after updates (panorama `test_scan_session.py` / `test_background.py`).

use super::*;

pub(crate) fn set_mtime(path: &Path, t: f64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(UNIX_EPOCH + Duration::from_secs_f64(t))
        .expect("set the mtime");
}

/// The newest time read from agent `id`'s file: `None` while it is unread.
pub(crate) fn read_ts(s: &Scanner, id: &str) -> Option<f64> {
    s.session().last_ts(id).map(Ts::secs)
}

/// panorama `test_incremental_reads_partial_lines_and_rewrites`.
#[test]
fn an_update_reads_partial_lines_and_a_rewrite_rebuilds() {
    let mut sc = Sc::new("incremental");
    let call = bg_agent(&mut sc, "a1", "x");
    let mut s = sc.load();
    let status = |s: &Scanner| {
        s.session()
            .status_of("a1", Ts(B + 4.0 * M), &NoProcs)
            .status
    };
    assert_eq!(status(&s), Status::Run);
    let old = std::fs::read_to_string(&sc.main.path).unwrap();
    sc.main
        .notify(B + 3.0 * M, "a1", &call, "completed", &["queue"]);
    let full = sc.main.lines.join("\n") + "\n";
    std::fs::write(&sc.main.path, &full[..old.len() + 40]).unwrap(); // the notice is half written
    s.update(None);
    assert_eq!(status(&s), Status::Run);
    std::fs::write(&sc.main.path, &full).unwrap();
    s.update(None);
    assert_eq!(status(&s), Status::Done);
    assert_eq!(s.session().notices("a1").len(), 1);
    std::fs::write(&sc.main.path, &old).unwrap(); // rewritten shorter: rebuilt from the new content
    s.update(None);
    assert!(s.session().notices("a1").is_empty());
    assert_eq!(status(&s), Status::Run);
}

/// panorama `test_old_agent_files_are_skipped_until_they_change`.
#[test]
fn an_old_agent_file_is_skipped_until_it_changes() {
    let mut sc = Sc::new("skip-old");
    bg_agent(&mut sc, "a1", "x");
    let mut s = sc.write_all();
    let path = sc.agents["a1"].0.path.clone();
    set_mtime(&path, B);
    s.update(Some(Ts(B + 3600.0)));
    assert_eq!(read_ts(&s, "a1"), None); // not read
    sc.tx("a1").bash(B + 7200.0, "again", false);
    Sc::write(&path, &sc.agents["a1"].0.lines);
    s.update(Some(Ts(B + 3600.0)));
    assert_eq!(read_ts(&s, "a1"), Some(B + 7201.0));
}

/// A file once read is followed whatever its mtime says (the `since` skip is
/// for files never read).
#[test]
fn a_file_once_read_is_followed_even_with_an_old_mtime() {
    let mut sc = Sc::new("followed");
    bg_agent(&mut sc, "a1", "x");
    let mut s = sc.write_all();
    s.update(Some(Ts(B + 3600.0))); // written now: read
    assert_eq!(read_ts(&s, "a1"), Some(B));
    sc.tx("a1").bash(B + 2.0 * M, "more", false);
    let path = sc.agents["a1"].0.path.clone();
    Sc::write(&path, &sc.agents["a1"].0.lines);
    set_mtime(&path, B); // appended, but its mtime is old (a restored or copied file)
    s.update(Some(Ts(B + 3600.0)));
    assert_eq!(read_ts(&s, "a1"), Some(B + 2.0 * M + 1.0));
}

/// panorama `test_background.py::test_an_old_agent_file_is_still_read_when_its_agent_never_ended`.
#[test]
fn an_old_agent_file_is_still_read_when_its_agent_never_ended() {
    let mut sc = Sc::new("old-bg");
    bg_agent(&mut sc, "a4", "x");
    sc.tx("a4").bash_background(B + M, "ab4");
    let done = bg_agent(&mut sc, "a5", "y");
    sc.tx("a5").bash_background(B + M, "ab5"); // ended: its background launch does not matter
    sc.main
        .notify(B + 2.0 * M, "a5", &done, "completed", &["queue"]);
    let mut s = sc.write_all();
    for id in ["a4", "a5"] {
        set_mtime(&sc.agents[id].0.path, B + 2.0 * M);
    }
    s.update(Some(Ts(B + 3.0 * 3600.0))); // both files are older than the window
    assert_eq!(
        s.session()
            .status_of("a4", Ts(B + 4.0 * 3600.0), &NoProcs)
            .status,
        Status::Bg
    );
    assert_eq!(read_ts(&s, "a5"), None); // a finished one is still skipped
    // read once per file version: a second update does not read it again
    s.update(Some(Ts(B + 3.0 * 3600.0)));
    assert_eq!(read_ts(&s, "a5"), None);
}

/// panorama `test_l2_hook_data_survives_a_rebuild`.
#[test]
fn hook_records_survive_a_rebuild() {
    let mut sc = Sc::new("hooks-rebuild");
    bg_agent(&mut sc, "a1", "x");
    sc.hook("stop", "a1", B + 2.0 * M, 4242, "s1");
    let mut s = sc.load();
    let got = |s: &Scanner| {
        let st = s.session().status_of("a1", Ts(B + 3.0 * M), &NoProcs);
        (st.status, st.ended, st.inferred)
    };
    let want = (Status::Done, Some(Ts(B + 2.0 * M)), false);
    assert_eq!(got(&s), want);
    let full = std::fs::read_to_string(&sc.main.path).unwrap();
    let first = &full[..=full.find('\n').unwrap()];
    std::fs::write(&sc.main.path, first).unwrap(); // rewritten shorter: a rebuild
    s.update(None);
    assert_eq!(got(&s), want);
    std::fs::write(&sc.main.path, &full).unwrap(); // and back
    s.update(None);
    assert_eq!(got(&s), want);
}

/// panorama `test_meta_read_mid_write_is_retried`.
#[test]
fn a_meta_read_mid_write_is_retried() {
    let mut sc = Sc::new("meta-retry");
    bg_agent(&mut sc, "a1", "x");
    let mut s = sc.write_all();
    let meta = sc.agents["a1"].0.path.with_file_name("agent-a1.meta.json");
    let good = std::fs::read_to_string(&meta).unwrap();
    std::fs::write(&meta, &good[..10]).unwrap();
    set_mtime(&meta, B);
    s.update(None);
    assert!(s.session().agent("a1").unwrap().meta.is_empty());
    std::fs::write(&meta, &good).unwrap();
    set_mtime(&meta, B + 1.0);
    s.update(None);
    assert_eq!(s.session().agent("a1").unwrap().meta["description"], "x");
}

/// `meta.json` and run summaries are read as Python's `json.load` reads
/// them: a lone UTF-16 surrogate escape is accepted, a UTF-8 BOM is not.
#[test]
fn metas_and_summaries_take_a_lone_surrogate_like_python() {
    let mut sc = Sc::new("meta-surrogate");
    let call = bg_agent(&mut sc, "a1", "x");
    let tx = sc.agent(
        "b1",
        json!({"description": "y", "toolUseId": "toolu_b"}),
        "meta_background",
        None,
    );
    tx.prompt(B, "brief");
    sc.main.workflow(B, "wf_1", "task1");
    sc.journal("wf_1", "started", "w1", "Build");
    let mut s = sc.write_all();
    let meta = sc.agents["a1"].0.path.with_file_name("agent-a1.meta.json");
    std::fs::write(
        &meta,
        format!(r#"{{"description": "cut \ud83d", "toolUseId": "{call}"}}"#),
    )
    .unwrap();
    let bom = sc.agents["b1"].0.path.with_file_name("agent-b1.meta.json");
    std::fs::write(&bom, b"\xef\xbb\xbf{\"description\": \"y\"}").unwrap();
    let summary = sc.session_dir().join("workflows/wf_1.json");
    std::fs::create_dir_all(summary.parent().unwrap()).unwrap();
    std::fs::write(
        &summary,
        r#"{"runId": "wf_1", "status": "killed", "name": "a \udc00"}"#,
    )
    .unwrap();
    s.update(None);
    let sess = s.session();
    assert_eq!(
        sess.agent("a1").unwrap().meta["description"],
        "cut \u{fffd}"
    );
    assert_eq!(sess.parent_of("a1"), Some(Owner::Main));
    assert!(sess.agent("b1").unwrap().meta.is_empty());
    assert_eq!(
        sess.wf_summary("wf_1").map(|m| m["status"].clone()),
        Some(json!("killed"))
    );
}

/// A rewritten hook file is read again from the start: its old records are gone.
#[test]
fn a_rewritten_hook_file_forgets_its_old_records() {
    let mut sc = Sc::new("hooks-rewrite");
    bg_agent(&mut sc, "a1", "x");
    sc.hook("stop", "a1", B + 2.0 * M, 4242, "s1");
    sc.hook("stop", "a2", B + 2.0 * M, 4242, "s1");
    let mut s = sc.load();
    let status = |s: &Scanner| {
        s.session()
            .status_of("a1", Ts(B + 3.0 * M), &NoProcs)
            .status
    };
    assert_eq!(status(&s), Status::Done);
    let hooks = sc.root.join("hooks").join(format!("{SID}.jsonl"));
    let kept = sc.hooks[1].clone();
    Sc::write(&hooks, &[kept]); // rewritten shorter, without a1's stop
    s.update(None);
    assert_eq!(status(&s), Status::Run);
}

/// panorama keeps `Session.agents` in discovery order across updates
/// (collect.py's `workflow_thread` and `children` read it in that order).
#[test]
fn agents_keep_their_discovery_order_across_updates() {
    let mut sc = Sc::new("agent-order");
    bg_agent(&mut sc, "b1", "first");
    let mut s = sc.load();
    bg_agent(&mut sc, "a1", "later");
    let _ = sc.write_all();
    s.update(None);
    let ids: Vec<&str> = s.session().agents().map(|(id, _)| id).collect();
    assert_eq!(ids, ["b1", "a1"]);
    assert_eq!(s.session().children(&Owner::Main), ["b1", "a1"]);
}
