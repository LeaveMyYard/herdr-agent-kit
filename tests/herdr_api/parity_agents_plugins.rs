//! Agent, plugin and notification calls send what herdr's CLI sends, and
//! read the recorded replies; `start_agent` waits for a ready agent.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use herdr_agent_kit::herdr_api::fake::{self, FakeHerdr, Reply};
use herdr_agent_kit::herdr_api::{
    AgentStatus, Direction, HerdrError, OpenPluginPane, Placement, calls,
};
use serde_json::{Value, json};

use crate::support::*;

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
