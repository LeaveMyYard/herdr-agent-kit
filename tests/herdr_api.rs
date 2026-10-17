//! `herdr_api` against `FakeHerdr` and `CliStub` only: never a real herdr.
//!
//! The parity tests take each recorded CLI or socket exchange of
//! `fixtures/herdr` and check that the typed call sends exactly the request
//! herdr's own CLI sent, and reads the recorded reply, through both clients.

use std::fmt::Debug;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use herdr_agent_kit::herdr_api::fake::{self, CliReply, CliStub, FakeHerdr, Reply};
use herdr_agent_kit::herdr_api::{
    AgentStatus, Call, Direction, EventKind, Events, Herdr, HerdrError, HerdrEvent, MoveTo,
    OpenPluginPane, PaneId, Placement, Sanitized, ServerId, TabId, WorkspaceId, blocking, calls,
};
use serde_json::{Value, json};

// -- the recorded exchanges -------------------------------------------------------

/// The recorded `(request params, reply frame)` of `method` in one record.
fn exchange(file: &str, seq: u64, method: &str) -> (Value, Value) {
    let record = fake::recording(file, seq).unwrap_or_else(|| panic!("{file}:{seq} recorded"));
    fake::exchanges(&record)
        .into_iter()
        .find(|(req, _)| req["method"] == method)
        .map(|(req, reply)| (req.get("params").cloned().unwrap_or(Value::Null), reply))
        .unwrap_or_else(|| panic!("{file}:{seq} has no {method}"))
}

fn recorded_param(file: &str, seq: u64, method: &str, pointer: &str) -> String {
    exchange(file, seq, method)
        .0
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

/// What a reply frame means to a call.
fn reply_of(frame: &Value) -> Result<Value, HerdrError> {
    match frame.get("error") {
        Some(e) => Err(HerdrError::Refused {
            code: e["code"].as_str().unwrap().to_owned(),
            message: e["message"].as_str().unwrap().to_owned(),
        }),
        None => Ok(frame["result"].clone()),
    }
}

/// The call sends the recorded request, and both clients read what the
/// recorded reply means. Returns that result.
async fn parity<T: PartialEq + Debug>(
    call: Call<T>,
    file: &str,
    seq: u64,
) -> Result<T, HerdrError> {
    let (params, frame) = exchange(file, seq, call.method());
    assert_eq!(
        call.params(),
        &params,
        "{file}:{seq} {} params",
        call.method()
    );
    let expected = call.parse(reply_of(&frame));
    let fake = FakeHerdr::recorded();
    fake.replay(file, seq).replay(file, seq); // once per client
    let got = fake.client().send(call.clone()).await;
    assert_eq!(got, expected, "{file}:{seq} async");
    let got = fake.blocking_client().send(call.clone());
    assert_eq!(got, expected, "{file}:{seq} blocking");
    let sent = fake.calls();
    assert_eq!(sent.len(), 2, "every request is recorded");
    for (method, p) in sent {
        assert_eq!((method.as_str(), &p), (call.method(), &params));
    }
    expected
}

fn p(id: &str) -> PaneId {
    PaneId::new(id)
}

fn t(id: &str) -> TabId {
    TabId::new(id)
}

fn text(s: &str) -> Sanitized {
    Sanitized::new(s)
}

#[tokio::test(flavor = "current_thread")]
async fn tabs_and_workspaces_send_what_herdrs_cli_sends() {
    let snap = parity(calls::snapshot(), "api.jsonl", 0).await.unwrap();
    assert_eq!(snap.agents[0].agent.as_deref(), Some("claude"));
    assert_eq!(snap.agents[0].agent_status, AgentStatus::Idle);
    let snap = parity(calls::snapshot(), "api.jsonl", 1).await.unwrap();
    assert_eq!(snap.panes[1].tokens["inbox_q"], "?2");
    let ws = parity(calls::workspaces(), "workspace.jsonl", 1)
        .await
        .unwrap();
    assert_eq!(ws[0].workspace_id, Some(WorkspaceId::new("w1")));
    let tabs = parity(calls::tabs(&WorkspaceId::new("w1")), "tab.jsonl", 2)
        .await
        .unwrap();
    assert_eq!(tabs.len(), 3);
    let tab = parity(calls::tab(&t("w1:t2")), "tab.jsonl", 3)
        .await
        .unwrap();
    assert_eq!(tab.label.as_deref(), Some("Agent"));
    let tab = parity(
        calls::rename_tab(&t("w1:t3"), &text("\u{2753} Other")),
        "tab.jsonl",
        4,
    )
    .await;
    assert_eq!(
        tab.unwrap().unwrap().label.as_deref(),
        Some("\u{2753} Other")
    );
    parity(calls::focus_tab(&t("w1:t3")), "tab.jsonl", 5)
        .await
        .unwrap();
    parity(calls::close_tab(&t("w1:t5")), "tab.jsonl", 6)
        .await
        .unwrap();
    for (seq, focus) in [(7, true), (8, false)] {
        let cwd = recorded_param("tab.jsonl", seq, "tab.create", "/cwd");
        let label = recorded_param("tab.jsonl", seq, "tab.create", "/label");
        let call = calls::create_tab(&text(&label), Some(Path::new(&cwd)), focus, None);
        parity(call, "tab.jsonl", seq).await.unwrap();
    }
    let cwd = recorded_param("tab.jsonl", 0, "tab.create", "/cwd");
    let w1 = WorkspaceId::new("w1");
    let call = calls::create_tab(&text("Agent"), Some(Path::new(&cwd)), true, Some(&w1));
    parity(call, "tab.jsonl", 0).await.unwrap();
    let tabs = parity(calls::move_tab(&t("w1:t3"), 0), "socket.jsonl", 4)
        .await
        .unwrap();
    assert_eq!(tabs[0].tab_id, Some(t("w1:t3")));
}

#[tokio::test(flavor = "current_thread")]
async fn panes_send_what_herdrs_cli_sends() {
    let panes = parity(calls::panes(), "pane.jsonl", 3).await.unwrap();
    assert!(panes.iter().any(|p| p.focused));
    let pane = parity(calls::pane(&p("w1:p2")), "pane.jsonl", 4)
        .await
        .unwrap();
    assert_eq!(pane.tab_id, Some(t("w1:t2")));
    parity(calls::focus_pane(&p("w1:p2")), "socket.jsonl", 3)
        .await
        .unwrap();
    let call = calls::rename_pane(&p("w1:p3"), &text("capture pane"));
    parity(call, "pane.jsonl", 12).await.unwrap();
    let zoom = parity(calls::toggle_zoom(&p("w1:p3")), "pane.jsonl", 13)
        .await
        .unwrap();
    assert_eq!(zoom["reason"], "single_pane");
    let info = parity(calls::process_info(&p("w1:p2")), "pane.jsonl", 6)
        .await
        .unwrap();
    let procs = info.foreground_processes.unwrap();
    assert_eq!(procs[0].name.as_deref(), Some("claude"));
    parity(calls::process_info(&p("w1:p6")), "pane.jsonl", 18)
        .await
        .unwrap();
    let to = MoveTo::Tab {
        tab: t("w1:t2"),
        target_pane: Some(p("w1:p2")),
    };
    let moved = parity(calls::move_pane(&p("w1:p5"), &to, false), "pane.jsonl", 21)
        .await
        .unwrap();
    assert!(moved.changed);
    assert_eq!(moved.pane.unwrap().pane_id, Some(p("w1:p5")));
    let to = MoveTo::NewTab {
        workspace: Some(WorkspaceId::new("w1")),
        label: text("\u{25c6} panorama"),
    };
    parity(calls::move_pane(&p("w1:p5"), &to, false), "pane.jsonl", 23)
        .await
        .unwrap();
    let call = calls::swap_panes(&p("w1:p5"), &p("w1:p2"));
    assert!(parity(call, "pane.jsonl", 22).await.unwrap().swapped());
    let call = calls::swap_panes(&p("w1:p2"), &p("w1:p3"));
    let swap = parity(call, "errors.jsonl", 16).await.unwrap();
    assert!(!swap.swapped());
    assert_eq!(swap.reason.as_deref(), Some("cross_tab"));
    let count = text("?2");
    let call = calls::set_token(&p("w1:p2"), "herdr-inbox", "inbox_q", Some(&count));
    parity(call, "pane.jsonl", 9).await.unwrap();
    let call = calls::set_token(&p("w1:p2"), "herdr-inbox", "inbox_q", None);
    parity(call, "pane.jsonl", 10).await.unwrap();
}

/// `pane.read` answers the same text herdr's CLI printed (herdr-inbox reads
/// the CLI's stdout today).
#[tokio::test(flavor = "current_thread")]
async fn pane_read_is_the_text_the_cli_printed() {
    for (seq, lines, ansi) in [(7, 20, false), (8, 30, true)] {
        let call = calls::read_pane(&p("w1:p2"), lines, ansi);
        let read = parity(call, "pane.jsonl", seq).await.unwrap();
        let record = fake::recording("pane.jsonl", seq).unwrap();
        assert_eq!(read, record["stdout"].as_str().unwrap());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn layouts_send_what_panorama_sends() {
    let lay = parity(calls::pane_layout(&p("w1:p2")), "socket.jsonl", 0)
        .await
        .unwrap();
    assert_eq!(lay.tab_id, Some(t("w1:t2")));
    let lay = parity(calls::layout_export(&t("w1:t2")), "socket.jsonl", 1)
        .await
        .unwrap();
    assert!(lay.root.is_some());
    parity(
        calls::set_split_ratio(&t("w1:t2"), &[], 0.6),
        "socket.jsonl",
        2,
    )
    .await
    .unwrap();
    let root = exchange("socket.jsonl", 6, "layout.apply").0["root"].clone();
    let call = calls::apply_layout(&text("New agent"), false, root);
    let lay = parity(call, "socket.jsonl", 6).await.unwrap();
    assert_eq!(lay.tab_id, Some(t("w1:t5")));
    assert_eq!(lay.root.unwrap()["first"]["pane_id"], "w1:p7");
}

#[tokio::test(flavor = "current_thread")]
async fn agents_send_what_herdrs_cli_sends() {
    let agents = parity(calls::agents(), "agent.jsonl", 0).await.unwrap();
    assert_eq!(agents.len(), 1);
    let agent = parity(calls::agent(&p("w1:p2")), "agent.jsonl", 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (agent.agent_status, agent.focused),
        (AgentStatus::Idle, true)
    );
    assert_eq!(
        parity(calls::agent(&p("w1:p3")), "agent.jsonl", 2).await,
        Ok(None)
    );
    assert_eq!(
        parity(calls::agent_named("nobody"), "errors.jsonl", 8).await,
        Ok(None)
    );
    let named = parity(calls::agent_named("capture-agent"), "agent.jsonl", 6)
        .await
        .unwrap();
    assert_eq!(named.unwrap().name.as_deref(), Some("capture-agent"));
    parity(calls::focus_agent(&p("w1:p2")), "agent.jsonl", 3)
        .await
        .unwrap();
    let call = calls::prompt(&p("w1:p2"), &text("capture prompt text"));
    parity(call, "agent.jsonl", 4).await.unwrap();
    let call = calls::start_agent(&text("capture-agent"), "claude", &p("w1:p8"), 8000);
    assert_eq!(call.timeout(), Some(Duration::from_secs(13)));
    parity(call, "agent.jsonl", 5).await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn plugins_and_notifications_send_what_herdrs_cli_sends() {
    let plugins = parity(calls::plugins(), "plugin.jsonl", 2).await.unwrap();
    let inbox = plugins
        .iter()
        .find(|p| p.plugin_id.as_deref() == Some("leavemyyard.herdr-inbox"))
        .unwrap();
    assert!(inbox.plugin_root.is_some() && inbox.enabled);
    let call = calls::invoke_action("leavemyyard.herdr-inbox.open");
    parity(call, "plugin.jsonl", 4).await.unwrap();
    parity(calls::close_plugin_pane(&p("w1:p4")), "plugin.jsonl", 8)
        .await
        .unwrap();
    let call = calls::notify(
        &text("Capture title"),
        &text("capture body"),
        Some("request"),
    );
    let shown = parity(call, "notification.jsonl", 0).await.unwrap();
    assert_eq!(shown.reason.as_deref(), Some("disabled"));
    parity(calls::reload_config(), "server.jsonl", 0)
        .await
        .unwrap();
}

/// Every `plugin.pane.open` form the plugins use.
#[tokio::test(flavor = "current_thread")]
async fn plugin_panes_open_as_herdrs_cli_opens_them() {
    let cwd = |seq| {
        PathBuf::from(recorded_param(
            "plugin.jsonl",
            seq,
            "plugin.pane.open",
            "/cwd",
        ))
    };
    let panorama = |entry: &str| OpenPluginPane::new("leavemyyard.herdr-panorama", entry);
    // panorama open_view: a popup, no pane in the answer
    let mut open = panorama("panorama");
    open.cwd = Some(cwd(5));
    assert_eq!(
        parity(calls::open_plugin_pane(&open), "plugin.jsonl", 5).await,
        Ok(None)
    );
    // herdr-inbox open_popup
    let mut open = OpenPluginPane::new("leavemyyard.herdr-inbox", "inbox");
    open.cwd = Some(cwd(6));
    open.env = vec![("INBOX_FOCUS_PANE".into(), "w1:p2".into())];
    assert_eq!(
        parity(calls::open_plugin_pane(&open), "plugin.jsonl", 6).await,
        Ok(None)
    );
    // panorama open_view(split)
    let mut open = panorama("panorama");
    open.placement = Some(Placement::Split);
    open.direction = Some(Direction::Right);
    open.target_pane = Some(p("w1:p2"));
    open.cwd = Some(cwd(7));
    let pane = parity(calls::open_plugin_pane(&open), "plugin.jsonl", 7)
        .await
        .unwrap();
    assert_eq!(pane.unwrap().pane_id, Some(p("w1:p4")));
    // panorama open_tab_pane
    let mut open = panorama("panorama");
    open.placement = Some(Placement::Tab);
    open.cwd = Some(cwd(9));
    open.focus = false;
    open.env = vec![("PANORAMA_HOME".into(), "1".into())];
    let pane = parity(calls::open_plugin_pane(&open), "plugin.jsonl", 9)
        .await
        .unwrap();
    assert!(pane.is_some());
    // panorama open_panel
    let mut open = panorama("panel");
    open.placement = Some(Placement::Split);
    open.direction = Some(Direction::Right);
    open.target_pane = Some(p("w1:p2"));
    open.cwd = Some(cwd(10));
    open.focus = false;
    let pane = parity(calls::open_plugin_pane(&open), "plugin.jsonl", 10)
        .await
        .unwrap();
    assert!(pane.is_some());
    // panorama open_new_agent (socket)
    let (params, _) = exchange("socket.jsonl", 5, "plugin.pane.open");
    let mut open = panorama("new-agent");
    open.placement = Some(Placement::Popup);
    open.size = Some((84, 26));
    open.env = vec![(
        "PANORAMA_NEW_AGENT_CWD".into(),
        params["env"]["PANORAMA_NEW_AGENT_CWD"]
            .as_str()
            .unwrap()
            .into(),
    )];
    open.cwd = params.get("cwd").and_then(Value::as_str).map(PathBuf::from);
    assert_eq!(
        parity(calls::open_plugin_pane(&open), "socket.jsonl", 5).await,
        Ok(None)
    );
}

/// Every recorded refusal reads as `Refused` with herdr's code.
#[tokio::test(flavor = "current_thread")]
async fn recorded_refusals_keep_herdrs_codes() {
    let unknown = p("w9-p99");
    let gone_tab = t("w9-t99");
    let x = text("x");
    let one = text("?1");
    let e = "errors.jsonl";
    let cases: Vec<(u64, Result<(), HerdrError>)> = vec![
        (1, parity(calls::pane(&unknown), e, 1).await.map(drop)),
        (3, parity(calls::tab(&gone_tab), e, 3).await.map(drop)),
        (
            4,
            parity(calls::rename_tab(&gone_tab, &x), e, 4)
                .await
                .map(drop),
        ),
        (5, parity(calls::focus_tab(&gone_tab), e, 5).await.map(drop)),
        (6, parity(calls::close_tab(&gone_tab), e, 6).await),
        (
            7,
            parity(calls::tabs(&WorkspaceId::new("w99")), e, 7)
                .await
                .map(drop),
        ),
        (
            9,
            parity(calls::focus_agent(&p("w1:p3")), e, 9)
                .await
                .map(drop),
        ),
        (
            10,
            parity(calls::prompt(&p("w1:p3"), &text("hello")), e, 10).await,
        ),
        (
            12,
            parity(calls::process_info(&unknown), e, 12).await.map(drop),
        ),
        (
            13,
            parity(calls::read_pane(&unknown, 20, false), e, 13)
                .await
                .map(drop),
        ),
        (
            14,
            parity(
                calls::set_token(&unknown, "herdr-inbox", "inbox_q", Some(&one)),
                e,
                14,
            )
            .await,
        ),
        (
            17,
            parity(calls::toggle_zoom(&unknown), e, 17).await.map(drop),
        ),
        (
            18,
            parity(calls::rename_pane(&unknown, &x), e, 18)
                .await
                .map(drop),
        ),
        (
            21,
            parity(calls::close_plugin_pane(&p("w1:p3")), e, 21).await,
        ),
        (
            22,
            parity(calls::invoke_action("nobody.none.open"), e, 22).await,
        ),
        (
            27,
            parity(calls::move_tab(&gone_tab, 0), e, 27).await.map(drop),
        ),
        (
            28,
            parity(calls::pane_layout(&unknown), e, 28).await.map(drop),
        ),
        (
            29,
            parity(calls::focus_pane(&unknown), e, 29).await.map(drop),
        ),
        (
            30,
            parity(calls::layout_export(&gone_tab), e, 30)
                .await
                .map(drop),
        ),
        (
            31,
            parity(calls::set_split_ratio(&t("w1:t3"), &[true; 3], 0.5), e, 31)
                .await
                .map(drop),
        ),
    ];
    for (seq, got) in cases {
        let record = fake::recording(e, seq).unwrap();
        let (_, frame) = fake::exchanges(&record).pop().unwrap();
        let code = frame["error"]["code"].as_str().unwrap();
        let err = got.expect_err("a refusal");
        assert!(err.is_code(code), "errors:{seq}: {err:?}");
        assert!(!err.maybe_done());
    }
    let to = MoveTo::Tab {
        tab: gone_tab.clone(),
        target_pane: None,
    };
    let err = parity(calls::move_pane(&p("w1:p3"), &to, false), e, 15)
        .await
        .unwrap_err();
    assert!(err.is_code("tab_not_found"));
    let call = calls::start_agent(&x, "claude", &unknown, 1000);
    assert!(
        parity(call, e, 11)
            .await
            .unwrap_err()
            .is_code("agent_pane_not_found")
    );
    let mut busy = OpenPluginPane::new("leavemyyard.herdr-inbox", "inbox");
    busy.cwd = Some(PathBuf::from(recorded_param(
        e,
        0,
        "plugin.pane.open",
        "/cwd",
    )));
    let err = parity(calls::open_plugin_pane(&busy), e, 0)
        .await
        .unwrap_err();
    assert!(err.is_code("ui_busy"));
    let none = OpenPluginPane::new("nobody.none", "x");
    let err = parity(calls::open_plugin_pane(&none), e, 19)
        .await
        .unwrap_err();
    assert!(err.is_code("plugin_not_found"));
}

// -- the transport ---------------------------------------------------------------

/// A private directory of this test's own (removed by the test).
fn scratch_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let dir = std::env::temp_dir().join(format!("{tag}{}-{nanos:x}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test(flavor = "current_thread")]
async fn no_socket_and_a_dead_socket_are_not_running() {
    let dir = scratch_dir("hn");
    let missing = ServerId::new(dir.join("none"));
    assert_eq!(
        Herdr::new(missing.clone()).snapshot().await,
        Err(HerdrError::NotRunning)
    );
    assert_eq!(
        blocking::Herdr::new(missing).snapshot(),
        Err(HerdrError::NotRunning)
    );
    let dead = dir.join("dead");
    drop(std::os::unix::net::UnixListener::bind(&dead).unwrap()); // the file stays, nobody listens
    // a child another test forked just then holds the listener until its exec
    let started = Instant::now();
    while std::os::unix::net::UnixStream::connect(&dead).is_ok() {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    let dead = ServerId::new(&dead);
    assert_eq!(
        Herdr::new(dead.clone()).panes().await,
        Err(HerdrError::NotRunning)
    );
    assert_eq!(
        blocking::Herdr::new(dead).panes(),
        Err(HerdrError::NotRunning)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// A socket this process may not connect to is unreachable, not "gone"
/// (herdr-inbox tests/test_successor.py: only herdr's own word is "gone").
#[tokio::test(flavor = "current_thread")]
async fn a_forbidden_socket_is_unreachable_not_gone() {
    let fake = FakeHerdr::recorded();
    std::fs::set_permissions(fake.socket(), std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::os::unix::net::UnixStream::connect(fake.socket()).is_ok() {
        return; // root ignores the mode: nothing to test
    }
    let err = fake.client().panes().await.unwrap_err();
    assert!(matches!(err, HerdrError::Unreachable(_)), "{err:?}");
    assert!(!err.maybe_done());
    let err = fake.blocking_client().panes().unwrap_err();
    assert!(matches!(err, HerdrError::Unreachable(_)), "{err:?}");
}

/// panorama tests/test_tui.py::test_a_reply_that_is_missing_or_garbled_after_sending_is_no_reply
#[tokio::test(flavor = "current_thread")]
async fn a_missing_or_garbled_reply_after_sending_may_have_happened() {
    let fake = FakeHerdr::recorded();
    let h = fake.client();
    let b = fake.blocking_client();
    let tab = t("w1:t3");
    let replies = [
        Reply::Close,
        Reply::Raw(b"not json\n".to_vec()),
        Reply::Raw(b"[1]\n".to_vec()),
    ];
    for reply in replies {
        fake.on("tab.move", reply.clone());
        let a = h.move_tab(&tab, 0).await.unwrap_err();
        let s = b.move_tab(&tab, 0).unwrap_err();
        for err in [a, s] {
            assert!(err.maybe_done(), "{reply:?}: {err:?}");
            match reply {
                Reply::Close => assert!(matches!(err, HerdrError::NoReply(_))),
                _ => assert!(matches!(err, HerdrError::Garbled(_))),
            }
        }
    }
    assert_eq!(fake.params_of("tab.move").len(), 6, "each one went out");
}

#[tokio::test(flavor = "current_thread")]
async fn an_unanswered_request_times_out_as_no_reply() {
    let fake = FakeHerdr::recorded();
    fake.on("agent.prompt", Reply::Hang);
    let limit = Duration::from_millis(200);
    let hi = text("hi");
    let started = Instant::now();
    let err = fake
        .client()
        .with_timeout(limit)
        .prompt(&p("w1:p2"), &hi)
        .await;
    assert_eq!(err, Err(HerdrError::NoReply("timed out".into())));
    let err = fake
        .blocking_client()
        .with_timeout(limit)
        .prompt(&p("w1:p2"), &hi);
    assert_eq!(err, Err(HerdrError::NoReply("timed out".into())));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test(flavor = "current_thread")]
async fn once_replies_come_first_then_the_recordings() {
    let fake = FakeHerdr::recorded();
    fake.once("agent.get", Reply::error("agent_not_found", "gone"));
    let h = fake.client().with_client("herdr-inbox");
    assert_eq!(h.agent(&p("w1:p2")).await, Ok(None));
    assert!(h.agent(&p("w1:p2")).await.unwrap().is_some());
    let ids: Vec<Value> = fake.requests().into_iter().map(|r| r.id).collect();
    assert_eq!(ids, [json!("herdr-inbox:1"), json!("herdr-inbox:1")]);
}

#[test]
fn the_fake_answers_a_garbled_line_like_herdr_and_records_it() {
    use std::io::{BufRead, BufReader, Write};
    let fake = FakeHerdr::start();
    let mut conn = std::os::unix::net::UnixStream::connect(fake.socket()).unwrap();
    conn.write_all(b"{not json\n").unwrap();
    let mut line = String::new();
    BufReader::new(&conn).read_line(&mut line).unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    let got = (reply["id"].as_str(), reply["error"]["code"].as_str());
    assert_eq!(got, (Some(""), Some("invalid_request")));
    let req = fake.requests().pop().unwrap();
    assert_eq!((req.method.as_str(), req.line.as_str()), ("", "{not json"));
    // nothing scripted or recorded: refused, never a made-up success
    let err = fake.blocking_client().panes().unwrap_err();
    assert!(err.is_code("invalid_request"));
}

// -- events --------------------------------------------------------------------------

async fn next(events: &mut Events) -> HerdrEvent {
    tokio::time::timeout(Duration::from_secs(5), events.next())
        .await
        .expect("an event within 5 s")
}

fn focus_kinds() -> [EventKind; 2] {
    [EventKind::TAB_FOCUSED, EventKind::PANE_FOCUSED]
}

#[test]
fn the_subscription_is_the_one_panorama_sends() {
    let (params, _) = exchange("events.jsonl", 2, "events.subscribe");
    let h = Herdr::new(ServerId::new("/nowhere")).with_client("panorama");
    let events = h.events(&focus_kinds());
    let sent: Value = serde_json::from_slice(events.request()).unwrap();
    let want = json!({"id": "panorama:events", "method": "events.subscribe", "params": params});
    assert_eq!(sent, want);
}

/// panorama tests/test_follow.py::test_event_stream_yields_focus_events_and_reconnects
/// and ::test_r12f_l1_events_marks_connect_and_disconnect
#[tokio::test(flavor = "current_thread")]
async fn events_mark_connect_and_loss_and_reconnect() {
    let fake = FakeHerdr::recorded();
    let mut events = fake.client().events(&focus_kinds());
    assert_eq!(next(&mut events).await, HerdrEvent::Connected);
    assert!(fake.wait_for_subscribers(1, Duration::from_secs(5)));
    // the recorded events, as herdr sent them, each after a line that is not JSON
    let record = fake::recording("events.jsonl", 2).unwrap();
    let recorded: Vec<Value> = record["replies"].as_array().unwrap()[1..].to_vec();
    for frame in &recorded {
        fake.send_line("not json");
        fake.send_line(&frame.to_string());
    }
    for frame in &recorded {
        let HerdrEvent::Event(e) = next(&mut events).await else {
            panic!("an event")
        };
        assert_eq!(Value::Object(e.data.clone()), frame["data"]);
        assert_eq!(e.kind, frame["event"].as_str().unwrap());
    }
    fake.drop_subscribers();
    assert_eq!(next(&mut events).await, HerdrEvent::Lost);
    assert_eq!(
        next(&mut events).await,
        HerdrEvent::Connected,
        "after a reconnect"
    );
    assert!(fake.wait_for_subscribers(1, Duration::from_secs(5)));
    fake.emit(json!({"type": "tab_focused", "tab_id": "w1:t1", "workspace_id": "w1"}));
    let HerdrEvent::Event(e) = next(&mut events).await else {
        panic!("an event")
    };
    assert_eq!(e.tab_id(), Some(t("w1:t1")));
    let subs = fake.params_of("events.subscribe");
    assert_eq!(subs.len(), 2);
    let want = json!({"subscriptions": [{"type": "tab.focused"}, {"type": "pane.focused"}]});
    assert_eq!(subs[0], want);
}

/// A refused subscription is not live: no `Connected`, and herdr keeps the
/// connection open, so nothing more comes (panorama's behaviour).
#[tokio::test(flavor = "current_thread")]
async fn a_refused_subscription_is_not_live() {
    let fake = FakeHerdr::recorded();
    let (_, frame) = exchange("errors.jsonl", 33, "events.subscribe");
    let (code, message) = (
        frame["error"]["code"].as_str(),
        frame["error"]["message"].as_str(),
    );
    fake.once(
        "events.subscribe",
        Reply::error(code.unwrap(), message.unwrap()),
    );
    let mut events = fake.client().events(&[EventKind::new("no.such.event")]);
    let quiet = tokio::time::timeout(Duration::from_millis(300), events.next()).await;
    assert!(quiet.is_err(), "nothing: {quiet:?}");
    assert_eq!(fake.subscribers(), 0);
}

/// No server: the stream keeps trying, quietly (no `Lost` for a stream that
/// was never live).
#[tokio::test(flavor = "current_thread")]
async fn events_without_a_server_stay_quiet() {
    let dir = scratch_dir("he");
    let mut events = Herdr::new(ServerId::new(dir.join("none"))).events(&focus_kinds());
    let quiet = tokio::time::timeout(Duration::from_millis(300), events.next()).await;
    assert!(quiet.is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

// -- the CLI path --------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn the_cli_path_reads_the_recorded_answers() {
    let stub = CliStub::recorded();
    let h = Herdr::new(ServerId::new("/nowhere")).with_bin(stub.bin());
    let b = h.blocking();
    assert_eq!(h.machine_list().await, Ok(vec![]));
    assert_eq!(b.machine_list(), Ok(vec![]));
    let unknown = "error: unknown machine 'nosuch'; use `herdr machine list`";
    let errs = [
        h.machine_snapshot("nosuch").await.unwrap_err(),
        b.machine_snapshot("nosuch").unwrap_err(),
    ];
    for err in errs {
        let want = HerdrError::Refused {
            code: String::new(),
            message: unknown.into(),
        };
        assert_eq!(err, want);
    }
    let err = h
        .machine_focus_agent("nosuch", &p("w1:p2"))
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), unknown);
    let record = fake::recording("plugin.jsonl", 3).unwrap();
    let dir = PathBuf::from(record["stdout"].as_str().unwrap().trim());
    let got = h.plugin_config_dir("leavemyyard.herdr-panorama").await;
    assert_eq!(got, Ok(Some(dir.clone())));
    assert_eq!(
        b.plugin_config_dir("leavemyyard.herdr-panorama"),
        Ok(Some(dir))
    );
    let other = h.plugin_config_dir("nobody.none").await.unwrap();
    assert!(other.is_some(), "herdr answers any id");
    let calls = stub.calls();
    assert_eq!(calls[0], ["machine", "list", "--json"]);
    assert_eq!(calls[2], ["--machine", "nosuch", "api", "snapshot"]);
}

/// panorama tests/test_herdr.py::test_snapshot_and_focus_through_the_fake: every
/// CLI argument goes through clean().
#[tokio::test(flavor = "current_thread")]
async fn cli_arguments_are_cleaned() {
    let mut stub = CliStub::new();
    stub.on(
        &["--machine", "m1", "agent", "focus", "w1:p1[2J"],
        CliReply::ok(""),
    );
    let h = Herdr::new(ServerId::new("/nowhere")).with_bin(stub.bin());
    h.machine_focus_agent("m1", &p("w1:p1\x1b[2J"))
        .await
        .unwrap();
    assert_eq!(
        stub.calls(),
        [["--machine", "m1", "agent", "focus", "w1:p1[2J"]]
    );
}

/// panorama tests/test_herdr.py::test_missing_binary_is_an_error_not_an_exception
/// and herdr-inbox tests/test_successor.py::test_server_gone_without_a_herdr_binary_is_not_gone
#[tokio::test(flavor = "current_thread")]
async fn a_missing_binary_is_unreachable() {
    let h = Herdr::new(ServerId::new("/nowhere")).with_bin("/nonexistent/herdr");
    assert!(matches!(
        h.machine_list().await,
        Err(HerdrError::Unreachable(_))
    ));
    let got = h.blocking().plugin_config_dir("x");
    assert!(matches!(got, Err(HerdrError::Unreachable(_))));
}

#[tokio::test(flavor = "current_thread")]
async fn a_slow_cli_times_out_and_server_not_running_is_not_running() {
    let mut stub = CliStub::new();
    let slow = CliReply {
        delay: Some(Duration::from_secs(3)),
        ..CliReply::ok("[]")
    };
    stub.on(&["machine", "list", "--json"], slow);
    let h = Herdr::new(ServerId::new("/nowhere"))
        .with_bin(stub.bin())
        .with_timeout(Duration::from_millis(300));
    let started = Instant::now();
    assert!(matches!(
        h.machine_list().await,
        Err(HerdrError::NoReply(_))
    ));
    assert!(matches!(
        h.blocking().machine_list(),
        Err(HerdrError::NoReply(_))
    ));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    // the stderr herdr-inbox tests/test_successor.py's fake herdr prints for a gone server
    let gone = CliReply {
        stderr:
            r#"{"id": "cli:machine:list", "error": {"code": "server_not_running", "message": "x"}}"#
                .into(),
        exit: 1,
        ..CliReply::default()
    };
    stub.on(&["machine", "list", "--json"], gone);
    assert_eq!(h.machine_list().await, Err(HerdrError::NotRunning));
}

// -- review round 1 ------------------------------------------------------------------

/// Review B1: `next()` sits in a `select!` beside a fast tick (plan §2.5);
/// each tick drops the pending `next()`. The reconnect wait must survive
/// that, or the stream never comes back after a loss.
#[tokio::test(flavor = "current_thread")]
async fn events_reconnect_inside_a_select_with_a_fast_tick() {
    let fake = FakeHerdr::recorded();
    let mut events = fake.client().events(&focus_kinds());
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let mut seen = Vec::new();
    let started = Instant::now();
    while seen.iter().filter(|e| **e == HerdrEvent::Connected).count() < 2 {
        assert!(started.elapsed() < Duration::from_secs(5), "seen {seen:?}");
        tokio::select! {
            event = events.next() => {
                if event == HerdrEvent::Connected && seen.is_empty() {
                    assert!(fake.wait_for_subscribers(1, Duration::from_secs(5)));
                    fake.drop_subscribers();
                }
                seen.push(event);
            }
            _ = tick.tick() => {}
        }
    }
    assert_eq!(
        seen,
        [
            HerdrEvent::Connected,
            HerdrEvent::Lost,
            HerdrEvent::Connected
        ]
    );
}

/// Review H1: `herdr agent start --timeout` waits until the agent is ready
/// (recording agent.jsonl:5: `agent.start` answers `launch_pending`, then
/// the CLI polls `agent.get` until `interactive_ready`). panorama types the
/// first prompt right after, so the Rust call must wait too.
#[tokio::test(flavor = "current_thread")]
async fn start_agent_waits_until_the_agent_is_ready() {
    let record = fake::recording("agent.jsonl", 5).unwrap();
    let frames = fake::exchanges(&record);
    let reply = |method: &str, ready: bool| {
        let (_, frame) = frames
            .iter()
            .find(|(req, rep)| {
                req["method"] == method
                    && (method == "agent.start"
                        || rep["result"]["agent"].get("interactive_ready").is_some() == ready)
            })
            .unwrap()
            .clone();
        Reply::Result(frame["result"].clone())
    };
    let name = text("capture-agent");
    for blocking in [false, true] {
        let fake = FakeHerdr::start();
        fake.once("agent.start", reply("agent.start", false));
        for _ in 0..2 {
            fake.once("agent.get", reply("agent.get", false));
        }
        fake.once("agent.get", reply("agent.get", true));
        let agent = if blocking {
            fake.blocking_client()
                .start_agent(&name, "claude", &p("w1:p8"), 8000)
        } else {
            fake.client()
                .start_agent(&name, "claude", &p("w1:p8"), 8000)
                .await
        }
        .unwrap();
        assert!(agent.is_ready());
        assert_eq!(agent.extra["interactive_ready"], true);
        let polls = fake.params_of("agent.get");
        assert_eq!(
            polls,
            vec![json!({"target": "capture-agent"}); 3],
            "blocking: {blocking}"
        );
        // never ready: gives up after timeout_ms, as "maybe started"
        fake.on("agent.start", reply("agent.start", false));
        fake.on("agent.get", reply("agent.get", false));
        let started = Instant::now();
        let err = if blocking {
            fake.blocking_client()
                .start_agent(&name, "claude", &p("w1:p8"), 500)
        } else {
            fake.client()
                .start_agent(&name, "claude", &p("w1:p8"), 500)
                .await
        }
        .unwrap_err();
        assert!(
            matches!(err, HerdrError::NoReply(_)) && err.maybe_done(),
            "{err:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}

/// Review M1: herdr confirmed the action (no error); an echo it left out is
/// not "maybe done".
#[tokio::test(flavor = "current_thread")]
async fn an_action_confirmed_without_its_echo_is_done() {
    let fake = FakeHerdr::start();
    for method in [
        "tab.focus",
        "tab.rename",
        "pane.focus",
        "pane.rename",
        "agent.focus",
        "pane.zoom",
        "layout.set_split_ratio",
    ] {
        fake.on(method, Reply::Result(json!({"type": "ok"})));
    }
    let h = fake.client();
    assert_eq!(h.focus_tab(&t("w1:t3")).await, Ok(None));
    assert_eq!(h.rename_tab(&t("w1:t3"), &text("x")).await, Ok(None));
    assert_eq!(h.focus_pane(&p("w1:p2")).await, Ok(None));
    assert_eq!(h.rename_pane(&p("w1:p2"), &text("x")).await, Ok(None));
    assert_eq!(h.focus_agent(&p("w1:p2")).await, Ok(None));
    assert_eq!(h.toggle_zoom(&p("w1:p2")).await, Ok(Default::default()));
    assert_eq!(h.set_split_ratio(&t("w1:t2"), &[], 0.5).await, Ok(None));
    // a read still needs its payload
    fake.on("tab.get", Reply::Result(json!({"type": "ok"})));
    assert!(matches!(
        h.tab(&t("w1:t3")).await,
        Err(HerdrError::Garbled(_))
    ));
}

/// Review L1: a herdr that stopped accepting (its backlog full) must not
/// hang the blocking client's connect.
#[cfg(target_os = "linux")]
#[test]
fn a_blocking_connect_to_a_wedged_server_times_out() {
    let dir = scratch_dir("hw");
    let path = dir.join("s");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    // fill the backlog: the filler blocks on the first connect that does not fit
    let filler = {
        let path = path.clone();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            while let Ok(s) = std::os::unix::net::UnixStream::connect(&path) {
                held.push(s);
            }
            held.len()
        })
    };
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    let err = blocking::Herdr::new(ServerId::new(&path))
        .with_timeout(Duration::from_millis(300))
        .panes()
        .unwrap_err();
    assert_eq!(err, HerdrError::Unreachable("connect timed out".into()));
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(listener);
    std::fs::remove_file(&path).unwrap();
    assert!(filler.join().unwrap() > 0);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Review L2: herdr exits, but a grandchild keeps its stdout open (ssh's
/// ControlPersist master under `--machine`): the blocking CLI path still
/// returns within its timeout.
#[test]
fn a_cli_whose_grandchild_holds_the_pipe_still_times_out() {
    let dir = scratch_dir("hg");
    let bin = dir.join("herdr");
    std::fs::write(&bin, "#!/bin/sh\nsleep 3 &\nprintf '[]'\nexit 0\n").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let h = blocking::Herdr::new(ServerId::new("/nowhere"))
        .with_bin(&bin)
        .with_timeout(Duration::from_millis(300));
    let started = Instant::now();
    assert!(matches!(h.machine_list(), Err(HerdrError::NoReply(_))));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
