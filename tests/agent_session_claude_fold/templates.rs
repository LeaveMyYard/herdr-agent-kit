//! Record templates: ISO times, notifications, tool blocks and results.

use super::*;

/// `t` as Claude Code writes it, with milliseconds.
pub(crate) fn iso(t: f64) -> String {
    let secs = t.floor() as i64;
    let ms = ((t - t.floor()) * 1000.0).round() as i64;
    common::iso(secs).replace(".000Z", &format!(".{ms:03}Z"))
}

pub(crate) fn notification(task: &str, call: &str, status: &str, tool_uses: u32) -> String {
    format!(
        "<task-notification>\n<task-id>{task}</task-id>\n<tool-use-id>{call}</tool-use-id>\n\
         <output-file>/tmp/tasks/{task}.output</output-file>\n<status>{status}</status>\n\
         <summary>Agent finished</summary>\n<result>report</result>\n\
         <usage><subagent_tokens>1000</subagent_tokens><tool_uses>{tool_uses}</tool_uses>\
         <duration_ms>1000</duration_ms></usage>\n</task-notification>"
    )
}

pub(crate) fn tool_block(
    mut rec: Map<String, Value>,
    name: &str,
    id: &str,
    input: Value,
) -> Map<String, Value> {
    let blocks = rec["message"]["content"].as_array().expect("blocks");
    let mut block = blocks
        .iter()
        .find(|b| b["type"] == "tool_use")
        .expect("a tool_use block")
        .clone();
    block["name"] = json!(name);
    block["id"] = json!(id);
    block["input"] = input;
    rec["message"]["content"] = json!([block]);
    rec
}

pub(crate) fn result_block(
    mut rec: Map<String, Value>,
    id: &str,
    text: &str,
    is_error: Option<bool>,
) -> Map<String, Value> {
    let blocks = rec["message"]["content"].as_array().expect("blocks");
    let mut block = blocks
        .iter()
        .find(|b| b["type"] == "tool_result")
        .expect("a tool_result block")
        .clone();
    block["tool_use_id"] = json!(id);
    block["content"] = json!([{"type": "text", "text": text}]);
    if let Some(e) = is_error {
        block["is_error"] = json!(e);
    }
    rec["message"]["content"] = json!([block]);
    rec
}

pub(crate) fn tur(rec: &mut Map<String, Value>, fields: Value) {
    let t = rec["toolUseResult"]
        .as_object_mut()
        .expect("a toolUseResult");
    for (k, v) in fields.as_object().expect("fields") {
        t.insert(k.clone(), v.clone());
    }
}
