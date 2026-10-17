//! [`Tx`]: one transcript, written from templates (scenario.Transcript).

use super::*;

/// One transcript, written from templates (scenario.Transcript).
pub(crate) struct Tx {
    pub(crate) path: PathBuf,
    pub(crate) agent: Option<String>,
    pub(crate) lines: Vec<String>,
    pub(crate) n: usize,
}

impl Tx {
    pub(crate) fn kind(&self, main: &str, sub: &str) -> Map<String, Value> {
        tpl(if self.agent.is_some() { sub } else { main })
    }

    pub(crate) fn tid(&mut self) -> String {
        self.n += 1;
        format!(
            "toolu_{}_{}",
            self.agent.as_deref().unwrap_or("main"),
            self.n
        )
    }

    pub(crate) fn add(&mut self, mut rec: Map<String, Value>, t: f64) {
        rec.insert("timestamp".into(), json!(iso(t)));
        rec.insert(
            "uuid".into(),
            json!(format!(
                "{:08x}-0000-4000-8000-000000000000",
                self.lines.len()
            )),
        );
        for key in ["sessionId", "session_id"] {
            if rec.contains_key(key) {
                rec.insert(key.into(), json!(SID));
            }
        }
        if rec.contains_key("cwd") {
            rec.insert("cwd".into(), json!(CWD));
        }
        if let Some(a) = &self.agent {
            rec.insert("agentId".into(), json!(a));
        }
        self.lines.push(serde_json::to_string(&rec).expect("json"));
    }

    pub(crate) fn prompt(&mut self, t: f64, text: &str) {
        let mut rec = self.kind("user_prompt_human", "sub_first_user");
        rec["message"]["content"] = json!(text);
        self.add(rec, t);
    }

    pub(crate) fn text(&mut self, t: f64, text: &str) {
        let mut rec = self.kind("assistant_text", "sub_assistant_text");
        let blocks = rec["message"]["content"].as_array().expect("blocks");
        let mut block = blocks
            .iter()
            .find(|b| b["type"] == "text")
            .expect("text")
            .clone();
        block["text"] = json!(text);
        rec["message"]["content"] = json!([block]);
        self.add(rec, t);
    }

    /// A Bash call and, unless `pending`, its result a second later.
    pub(crate) fn bash(&mut self, t: f64, description: &str, pending: bool) {
        let id = self.tid();
        let rec = self.kind("assistant_bash", "sub_assistant_bash");
        self.add(
            tool_block(
                rec,
                "Bash",
                &id,
                json!({"command": "true", "description": description}),
            ),
            t,
        );
        if !pending {
            let mut res = self.kind("result_bash", "sub_result_bash");
            tur(&mut res, json!({"stdout": "ok", "stderr": ""}));
            self.add(result_block(res, &id, "ok", Some(false)), t + 1.0);
        }
    }

    pub(crate) fn handback(&mut self, t: f64) {
        let id = self.tid();
        let rec = tpl("sub_assistant_handback");
        self.add(
            tool_block(rec, "SubagentHandback", &id, json!({"message": "done"})),
            t,
        );
    }

    /// An Agent call; a background one gets its `async_launched` result.
    pub(crate) fn launch(
        &mut self,
        t: f64,
        agent: &str,
        description: &str,
        background: bool,
    ) -> String {
        self.launch_as(t, agent, description, background, "general-purpose")
    }

    /// [`Tx::launch`] of a given `subagent_type`.
    pub(crate) fn launch_as(
        &mut self,
        t: f64,
        agent: &str,
        description: &str,
        background: bool,
        kind: &str,
    ) -> String {
        let id = self.tid();
        let mut input = json!({"description": description, "subagent_type": kind,
                               "model": "opus", "prompt": "brief"});
        if background {
            input["run_in_background"] = json!(true);
        }
        let rec = self.kind("assistant_agent", "sub_assistant_agent");
        self.add(tool_block(rec, "Agent", &id, input), t);
        if background {
            let mut res = self.kind("result_agent_async", "sub_result_agent_async");
            tur(
                &mut res,
                json!({"agentId": agent, "description": description, "status": "async_launched"}),
            );
            self.add(
                result_block(
                    res,
                    &id,
                    &format!("Async agent launched successfully.\nagentId: {agent}"),
                    None,
                ),
                t + 1.0,
            );
        }
        id
    }

    /// An erroring result of a spawning call that names its agent but has no
    /// status: a spawn result that is no acknowledgement.
    pub(crate) fn spawn_error(&mut self, t: f64, call: &str, agent: &str) {
        let mut res = tpl("result_agent_foreground");
        res["toolUseResult"] = json!({"agentId": agent, "totalToolUseCount": 1});
        self.add(
            result_block(res, call, "Error: the agent stopped", Some(true)),
            t,
        );
    }

    pub(crate) fn foreground_done(&mut self, t: f64, call: &str, agent: &str, error: bool) {
        let mut res = tpl("result_agent_foreground");
        tur(
            &mut res,
            json!({"agentId": agent, "status": "completed", "totalToolUseCount": 3}),
        );
        self.add(result_block(res, call, "report", Some(error)), t);
    }

    /// A `<task-notification>` in the given carriers (`queue`, `attachment`, `user`).
    pub(crate) fn notify(
        &mut self,
        t: f64,
        task: &str,
        call: &str,
        status: &str,
        carriers: &[&str],
    ) {
        let text = notification(task, call, status, 0);
        if carriers.contains(&"queue") && self.agent.is_none() {
            let mut q = tpl("queue_enqueue_notification");
            q["content"] = json!(text);
            self.add(q, t);
        }
        if carriers.contains(&"attachment") {
            let mut att = self.kind("attachment_queued_command", "sub_attachment_queued_command");
            att["attachment"]["prompt"] = json!(text);
            self.add(att, t + 0.01);
        }
        if carriers.contains(&"user") {
            let mut rec = tpl("user_prompt");
            rec.remove("origin");
            rec["message"]["content"] = json!(text);
            self.add(rec, t + 0.02);
        }
    }

    pub(crate) fn resume(&mut self, t: f64, agent: &str) {
        let id = self.tid();
        let rec = tpl("assistant_sendmessage");
        self.add(
            tool_block(
                rec,
                "SendMessage",
                &id,
                json!({"to": agent, "message": "continue"}),
            ),
            t,
        );
        let mut res = tpl("result_sendmessage_resume");
        tur(
            &mut res,
            json!({"success": true, "message": "Resuming", "resumedAgentId": agent}),
        );
        let text = res["toolUseResult"].to_string();
        self.add(result_block(res, &id, &text, None), t + 1.0);
    }

    pub(crate) fn workflow(&mut self, t: f64, run: &str, task: &str) {
        let id = self.tid();
        self.add(
            tool_block(
                tpl("assistant_workflow"),
                "Workflow",
                &id,
                json!({"script": "x"}),
            ),
            t,
        );
        let mut res = tpl("result_workflow");
        tur(
            &mut res,
            json!({"status": "async_launched", "taskId": task, "runId": run,
                             "workflowName": "wf", "taskType": "local_workflow"}),
        );
        self.add(
            result_block(
                res,
                &id,
                &format!("Workflow launched in background. Task ID: {task}"),
                None,
            ),
            t + 1.0,
        );
    }
}
