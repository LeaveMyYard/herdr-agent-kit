//! Every recorded refusal keeps herdr's error code.

use std::path::PathBuf;

use herdr_agent_kit::herdr_api::fake;
use herdr_agent_kit::herdr_api::{HerdrError, MoveTo, OpenPluginPane, WorkspaceId, calls};

use crate::support::*;

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
