//! Pane calls: `pane.list`, `pane.get`, `pane.focus`, `pane.rename`,
//! `pane.zoom`, `pane.read`, `pane.process_info`, `pane.move`, `pane.swap`
//! and `pane.report_metadata`.

use serde_json::{Map, Value, json};

use super::call::Call;
use super::reply::{echo, extra, field, list, pane_info, s, unit};
use crate::herdr_api::ids::{PaneId, TabId, WorkspaceId};
use crate::herdr_api::sanitize::{MAX_ARG, Sanitized, clean};
use crate::herdr_api::types::{Extra, MoveResult, PaneInfo, ProcessInfo, SwapResult};

/// `pane.list` (herdr-inbox `pane_tabs`, `focused_pane`; panorama
/// `tab_root_pane`).
pub fn panes() -> Call<Vec<PaneInfo>> {
    Call::new("pane.list", json!({}), |r| Ok(list(r?, "panes")))
}

/// `pane.get` (herdr-inbox `pane_identity`).
pub fn pane(pane: &PaneId) -> Call<PaneInfo> {
    Call::new("pane.get", json!({"pane_id": s(pane.as_str())}), pane_info)
}

/// `pane.focus` (panorama `focus_pane`).
pub fn focus_pane(pane: &PaneId) -> Call<Option<PaneInfo>> {
    Call::new("pane.focus", json!({"pane_id": s(pane.as_str())}), |r| {
        echo(r, "pane")
    })
}

/// `pane.rename` (panorama `rename_pane`).
pub fn rename_pane(pane: &PaneId, label: &Sanitized) -> Call<Option<PaneInfo>> {
    Call::new(
        "pane.rename",
        json!({"pane_id": s(pane.as_str()), "label": s(label.as_str())}),
        |r| echo(r, "pane"),
    )
}

/// `pane.zoom`, toggled (panorama `zoom_pane`). Answers herdr's `zoom` object
/// (empty when herdr echoed none).
pub fn toggle_zoom(pane: &PaneId) -> Call<Extra> {
    Call::new(
        "pane.zoom",
        json!({"pane_id": s(pane.as_str()), "mode": "toggle"}),
        |r| extra(Ok(echo::<Value>(r, "zoom")?.unwrap_or_default())),
    )
}

/// Python's universal newlines: `\r\n` and a lone `\r` read as `\n`.
pub fn universal_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `pane.read` of the visible screen: the last `lines` lines, plain text or
/// with ANSI styling (herdr-inbox `panel_focused`, `composer_state`).
///
/// Line ends are read as the Python clients read the CLI's output (text
/// mode, [`universal_newlines`]): herdr's ANSI format ends lines with
/// `\r\n`, which `composer_state` saw as `\n`.
pub fn read_pane(pane: &PaneId, lines: u16, ansi: bool) -> Call<String> {
    Call::new(
        "pane.read",
        json!({"pane_id": s(pane.as_str()), "source": "visible", "lines": lines,
               "format": if ansi { "ansi" } else { "text" }, "strip_ansi": true}),
        |r| {
            let read: Value = field(r?, "read")?;
            field::<String>(read, "text").map(|t| universal_newlines(&t))
        },
    )
}

/// `pane.process_info` (panorama `runs_panorama`, `claude_pid`).
pub fn process_info(pane: &PaneId) -> Call<ProcessInfo> {
    Call::new(
        "pane.process_info",
        json!({"pane_id": s(pane.as_str())}),
        |r| field(r?, "process_info"),
    )
}

/// Where [`move_pane`] puts a pane.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum MoveTo {
    /// Into a tab, split right (of `target_pane` when given). panorama
    /// `move_pane_to_tab`.
    Tab {
        /// The tab.
        tab: TabId,
        /// The pane to split.
        target_pane: Option<PaneId>,
    },
    /// Into a new tab (panorama `move_pane_new_tab`).
    NewTab {
        /// The workspace, or herdr's choice.
        workspace: Option<WorkspaceId>,
        /// The new tab's label.
        label: Sanitized,
    },
}

/// `pane.move`. The answer's `pane.pane_id` is the pane's id from now on.
pub fn move_pane(pane: &PaneId, to: &MoveTo, focus: bool) -> Call<MoveResult> {
    let destination = match to {
        MoveTo::Tab { tab, target_pane } => {
            let mut d = json!({"type": "tab", "tab_id": s(tab.as_str()), "split": "right"});
            if let Some(t) = target_pane {
                d["target_pane_id"] = s(t.as_str());
            }
            d
        }
        MoveTo::NewTab { workspace, label } => {
            let mut d = json!({"type": "new_tab", "label": s(label.as_str())});
            if let Some(ws) = workspace {
                d["workspace_id"] = s(ws.as_str());
            }
            d
        }
    };
    Call::new(
        "pane.move",
        json!({"pane_id": s(pane.as_str()), "destination": destination, "focus": focus}),
        |r| field(r?, "move_result"),
    )
}

/// `pane.swap` of two panes of one tab (panorama `swap_panes`). Across tabs
/// herdr does not refuse: it answers `changed: false` (see
/// [`SwapResult::swapped`]).
pub fn swap_panes(source: &PaneId, target: &PaneId) -> Call<SwapResult> {
    Call::new(
        "pane.swap",
        json!({"source_pane_id": s(source.as_str()), "target_pane_id": s(target.as_str())}),
        |r| field(r?, "swap"),
    )
}

/// `pane.report_metadata`: set the pane's token `key` to `value`, or clear it
/// with `None` (herdr-inbox `set_open_count`: `inbox_q` = `?N`).
pub fn set_token(pane: &PaneId, source: &str, key: &str, value: Option<&Sanitized>) -> Call<()> {
    let mut tokens = Map::new();
    tokens.insert(
        clean(key, MAX_ARG),
        value.map_or(Value::Null, |v| s(v.as_str())),
    );
    Call::new(
        "pane.report_metadata",
        json!({"pane_id": s(pane.as_str()), "source": s(source), "tokens": tokens,
               "clear_title": false, "clear_display_agent": false, "clear_state_labels": false}),
        unit,
    )
}
