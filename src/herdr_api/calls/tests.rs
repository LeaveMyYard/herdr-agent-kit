use std::time::Duration;

use serde_json::json;

use super::*;
use crate::herdr_api::{HerdrError, PaneId, Sanitized};

#[test]
fn every_string_param_goes_through_clean() {
    let call = prompt(&PaneId::new("w1:p1\x1b[2J"), &Sanitized::new("hi"));
    assert_eq!(call.params()["target"], "w1:p1[2J");
    let open = OpenPluginPane {
        env: vec![("K\n".into(), "v\x07".into())],
        ..OpenPluginPane::new("p", "e")
    };
    assert_eq!(open_plugin_pane(&open).params()["env"], json!({"K ": "v"}));
}

#[test]
fn no_agent_is_none_and_other_errors_stay_errors() {
    let call = agent(&PaneId::new("w1:p3"));
    let gone = HerdrError::Refused {
        code: AGENT_NOT_FOUND.into(),
        message: "x".into(),
    };
    assert_eq!(call.parse(Err(gone)).unwrap(), None);
    assert_eq!(call.parse(Ok(json!({}))).unwrap(), None);
    assert_eq!(
        call.parse(Err(HerdrError::NotRunning)),
        Err(HerdrError::NotRunning)
    );
}

#[test]
fn line_ends_read_as_in_python_text_mode() {
    assert_eq!(universal_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
}

#[test]
fn start_agent_waits_its_own_timeout_plus_five_seconds() {
    let call = start_agent(
        &Sanitized::new("a"),
        "claude",
        &PaneId::new("w1:p8"),
        60_000,
    );
    assert_eq!(call.timeout(), Some(Duration::from_secs(65)));
}
