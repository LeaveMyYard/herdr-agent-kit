//! Tab calls: `tab.list`, `tab.get`, `tab.create`, `tab.rename`,
//! `tab.focus`, `tab.move` and `tab.close`; and a tab's layout:
//! `pane.layout`, `layout.export`, `layout.set_split_ratio` and
//! `layout.apply`.

use std::path::Path;

use serde_json::{Value, json};

use super::call::Call;
use super::params::{path, s};
use super::reply::{echo, layout, list, tab_info, unit};
use crate::herdr_api::ids::{PaneId, TabId, WorkspaceId};
use crate::herdr_api::sanitize::Sanitized;
use crate::herdr_api::types::{LayoutInfo, TabInfo};

/// `tab.list` of one workspace (herdr-inbox `all_tabs`).
pub fn tabs(workspace: &WorkspaceId) -> Call<Vec<TabInfo>> {
    Call::new(
        "tab.list",
        json!({"workspace_id": s(workspace.as_str())}),
        |r| Ok(list(r?, "tabs")),
    )
}

/// `tab.get` (herdr-inbox `tab_label`).
pub fn tab(tab: &TabId) -> Call<TabInfo> {
    Call::new("tab.get", json!({"tab_id": s(tab.as_str())}), tab_info)
}

/// `tab.create`. Without `workspace` herdr picks one (panorama `create_tab`).
pub fn create_tab(
    label: &Sanitized,
    cwd: Option<&Path>,
    focus: bool,
    workspace: Option<&WorkspaceId>,
) -> Call<TabInfo> {
    let mut params = json!({"label": s(label.as_str()), "focus": focus});
    if let Some(cwd) = cwd {
        params["cwd"] = path(cwd);
    }
    if let Some(ws) = workspace {
        params["workspace_id"] = s(ws.as_str());
    }
    Call::new("tab.create", params, tab_info)
}

/// `tab.rename` (both plugins' `rename_tab`).
pub fn rename_tab(tab: &TabId, label: &Sanitized) -> Call<Option<TabInfo>> {
    Call::new(
        "tab.rename",
        json!({"tab_id": s(tab.as_str()), "label": s(label.as_str())}),
        |r| echo(r, "tab"),
    )
}

/// `tab.focus` (panorama `focus_tab`).
pub fn focus_tab(tab: &TabId) -> Call<Option<TabInfo>> {
    Call::new("tab.focus", json!({"tab_id": s(tab.as_str())}), |r| {
        echo(r, "tab")
    })
}

/// `tab.move` to `insert_index`; answers the workspace's tabs (panorama
/// `move_tab`).
pub fn move_tab(tab: &TabId, insert_index: u32) -> Call<Vec<TabInfo>> {
    Call::new(
        "tab.move",
        json!({"tab_id": s(tab.as_str()), "insert_index": insert_index}),
        |r| Ok(list(r?, "tabs")),
    )
}

/// `tab.close` (panorama `close_tab`).
pub fn close_tab(tab: &TabId) -> Call<()> {
    Call::new("tab.close", json!({"tab_id": s(tab.as_str())}), unit)
}

/// `pane.layout`: the layout of the pane's tab (panorama `pane_layout`).
pub fn pane_layout(pane: &PaneId) -> Call<LayoutInfo> {
    Call::new("pane.layout", json!({"pane_id": s(pane.as_str())}), layout)
}

/// `layout.export` of a tab (panorama `layout_export`).
pub fn layout_export(tab: &TabId) -> Call<LayoutInfo> {
    Call::new("layout.export", json!({"tab_id": s(tab.as_str())}), layout)
}

/// `layout.set_split_ratio`: one split of the tab (`path`: `false` = first
/// child, `true` = second, from the root); `ratio` is the first child's
/// share (panorama `set_split_ratio`).
pub fn set_split_ratio(tab: &TabId, path: &[bool], ratio: f64) -> Call<Option<LayoutInfo>> {
    Call::new(
        "layout.set_split_ratio",
        json!({"tab_id": s(tab.as_str()), "path": path, "ratio": ratio}),
        |r| echo(r, "layout"),
    )
}

/// `layout.apply`: a new tab with the split tree `root`, passed as given
/// (panorama `apply_panel_layout`, which caps the label at 200).
pub fn apply_layout(tab_label: &Sanitized, focus: bool, root: Value) -> Call<LayoutInfo> {
    Call::new(
        "layout.apply",
        json!({"tab_label": s(tab_label.as_str()), "focus": focus, "root": root}),
        layout,
    )
}
