//! Tab, workspace, pane and layout calls send what herdr's CLI (or
//! panorama) sends, and read the recorded replies.

use std::path::Path;

use herdr_agent_kit::herdr_api::fake::{self};
use herdr_agent_kit::herdr_api::{AgentStatus, MoveTo, WorkspaceId, calls};

use crate::support::*;

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
