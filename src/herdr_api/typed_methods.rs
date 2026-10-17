//! `typed_methods!`: the one list of typed methods both clients generate.

/// The typed methods, one per constructor in [`calls`](super::calls), generated for both
/// clients from this one list.
macro_rules! typed_methods {
    ($mac:ident) => {
        $mac! {
            snapshot() -> Snapshot;
            workspaces() -> Vec<WorkspaceInfo>;
            tabs(workspace: &WorkspaceId) -> Vec<TabInfo>;
            tab(tab: &TabId) -> TabInfo;
            create_tab(label: &Sanitized, cwd: Option<&std::path::Path>, focus: bool, workspace: Option<&WorkspaceId>) -> TabInfo;
            rename_tab(tab: &TabId, label: &Sanitized) -> Option<TabInfo>;
            focus_tab(tab: &TabId) -> Option<TabInfo>;
            move_tab(tab: &TabId, insert_index: u32) -> Vec<TabInfo>;
            close_tab(tab: &TabId) -> ();
            panes() -> Vec<PaneInfo>;
            pane(pane: &PaneId) -> PaneInfo;
            focus_pane(pane: &PaneId) -> Option<PaneInfo>;
            rename_pane(pane: &PaneId, label: &Sanitized) -> Option<PaneInfo>;
            toggle_zoom(pane: &PaneId) -> Extra;
            read_pane(pane: &PaneId, lines: u16, ansi: bool) -> String;
            process_info(pane: &PaneId) -> ProcessInfo;
            move_pane(pane: &PaneId, to: &MoveTo, focus: bool) -> MoveResult;
            swap_panes(source: &PaneId, target: &PaneId) -> SwapResult;
            pane_layout(pane: &PaneId) -> LayoutInfo;
            layout_export(tab: &TabId) -> LayoutInfo;
            set_split_ratio(tab: &TabId, path: &[bool], ratio: f64) -> Option<LayoutInfo>;
            apply_layout(tab_label: &Sanitized, focus: bool, root: serde_json::Value) -> LayoutInfo;
            set_token(pane: &PaneId, source: &str, key: &str, value: Option<&Sanitized>) -> ();
            agents() -> Vec<AgentInfo>;
            agent(pane: &PaneId) -> Option<AgentInfo>;
            agent_named(name: &str) -> Option<AgentInfo>;
            focus_agent(pane: &PaneId) -> Option<AgentInfo>;
            prompt(pane: &PaneId, text: &Sanitized) -> ();
            plugins() -> Vec<PluginInfo>;
            invoke_action(action_id: &str) -> ();
            open_plugin_pane(open: &OpenPluginPane) -> Option<PaneInfo>;
            close_plugin_pane(pane: &PaneId) -> ();
            notify(title: &Sanitized, body: &Sanitized, sound: Option<&str>) -> NotificationResult;
            reload_config() -> Extra;
        }
    };
}
pub(super) use typed_methods;
