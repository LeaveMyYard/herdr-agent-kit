use super::*;
use crate::agent_session::claude::wire::MARKERS;

#[test]
fn every_dispatched_tool_has_its_marker() {
    // MARKERS is panorama's byte for byte, so it spells the names itself; it
    // must name every tool the fold dispatches on, or a full-file pass skips
    // the line
    for tool in [AGENT, TASK, SEND_MESSAGE, MONITOR, TASK_STOP, KILL_SHELL] {
        let marker = format!(r#""name":"{tool}""#);
        assert!(MARKERS.contains(&marker.as_bytes()), "{tool}");
    }
}
