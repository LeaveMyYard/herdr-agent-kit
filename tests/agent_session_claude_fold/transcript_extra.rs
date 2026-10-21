//! More [`Tx`] and [`Sc`] records (scenario.py): titles, legacy prompts, raw lines, a Bash
//! call with its output, a foreground result with its tool count, a hook record without its
//! process.

use super::*;

impl Tx {
    /// A `custom_title` / `agent_name` / `ai_title` record: an undated metadata line.
    pub(crate) fn title(&mut self, kind: &str, value: &str) {
        let key = match kind {
            "custom_title" => "customTitle",
            "agent_name" => "agentName",
            _ => "aiTitle",
        };
        let mut rec = tpl(kind);
        rec.insert(key.into(), json!(value));
        if rec.contains_key("sessionId") {
            rec.insert("sessionId".into(), json!(SID));
        }
        self.lines.push(serde_json::to_string(&rec).expect("json"));
    }

    /// An operator prompt as older sessions wrote it: no `origin`.
    pub(crate) fn prompt_legacy(&mut self, t: f64, text: &str) {
        let mut rec = tpl("user_prompt");
        rec.remove("origin");
        rec["message"]["content"] = json!(text);
        self.add(rec, t);
    }

    /// A line as given.
    pub(crate) fn raw(&mut self, line: Value) {
        self.lines.push(line.to_string());
    }

    /// A Bash call whose result's output is `output`, a second later.
    pub(crate) fn bash_output(&mut self, t: f64, description: &str, output: &str) {
        let id = self.tid();
        let rec = self.kind("assistant_bash", "sub_assistant_bash");
        let input = json!({"command": "true", "description": description});
        self.add(tool_block(rec, "Bash", &id, input), t);
        let mut res = self.kind("result_bash", "sub_result_bash");
        tur(&mut res, json!({"stdout": output, "stderr": ""}));
        self.add(result_block(res, &id, output, Some(false)), t + 1.0);
    }

    /// A foreground agent's result with its `totalToolUseCount`.
    pub(crate) fn foreground_counted(
        &mut self,
        t: f64,
        call: &str,
        agent: &str,
        tools: u64,
        error: bool,
    ) {
        let mut res = tpl("result_agent_foreground");
        let fields = json!({"agentId": agent, "status": "completed", "totalToolUseCount": tools});
        tur(&mut res, fields);
        self.add(result_block(res, call, "report", Some(error)), t);
    }
}

impl Sc {
    /// A panorama hook record without the process `_append` adds (no pid, no start).
    pub(crate) fn hook_bare(&mut self, event: &str, agent: &str, ts: f64) {
        let rec = json!({"v": 1, "event": event, "agent": agent, "agent_type": "general-purpose", "ts": ts});
        self.hooks.push(rec.to_string());
    }

    /// A background agent `id` launched by main at `t`, its meta `meta` (plus its call).
    pub(crate) fn launched(&mut self, t: f64, id: &str, background: bool, meta: Value) -> String {
        let call = self.main.launch(t, id, "x", background);
        let mut m = meta.as_object().expect("meta").clone();
        m.insert("toolUseId".into(), json!(call));
        self.agent(id, Value::Object(m), "meta_background", None)
            .prompt(t, "brief");
        call
    }
}
