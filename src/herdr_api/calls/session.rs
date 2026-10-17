//! Session-wide calls: `session.snapshot`, `workspace.list`,
//! `notification.show` and `server.reload_config`.

use serde_json::json;

use super::call::Call;
use super::reply::{extra, field, list, s};
use crate::herdr_api::error::HerdrError;
use crate::herdr_api::sanitize::Sanitized;
use crate::herdr_api::types::{Extra, NotificationResult, Snapshot, WorkspaceInfo};

/// `session.snapshot`: the whole session (panorama `snapshot`).
pub fn snapshot() -> Call<Snapshot> {
    Call::new("session.snapshot", json!({}), |r| field(r?, "snapshot"))
}

/// `workspace.list` (herdr-inbox `all_tabs`).
pub fn workspaces() -> Call<Vec<WorkspaceInfo>> {
    Call::new("workspace.list", json!({}), |r| Ok(list(r?, "workspaces")))
}

/// `notification.show` (herdr-inbox `notify`, with sound `request`).
pub fn notify(
    title: &Sanitized,
    body: &Sanitized,
    sound: Option<&str>,
) -> Call<NotificationResult> {
    let mut params = json!({"title": s(title.as_str()), "body": s(body.as_str())});
    if let Some(sound) = sound {
        params["sound"] = s(sound);
    }
    Call::new("notification.show", params, |r| {
        serde_json::from_value(r?).map_err(|e| HerdrError::Garbled(e.to_string()))
    })
}

/// `server.reload_config` (herdr-inbox radar).
pub fn reload_config() -> Call<Extra> {
    Call::new("server.reload_config", json!({}), extra)
}
