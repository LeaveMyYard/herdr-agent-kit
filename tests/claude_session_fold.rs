//! The status rules of herdr-panorama's `scan/session.py` (its docstring,
//! lines 1-44), as a table: each row builds a session from captured
//! templates (`tests/common`, as panorama's `tests/scenario.py` does), reads
//! it with [`Scanner`] and asks [`Session::status_of`] for one agent at one
//! instant.
//!
//! The hook records are panorama's own v1 format (`hooks.py` `_append` /
//! `record`), not a Claude Code wire format.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use common::{BASE, CWD, SLUG, tmp_dir, tpl};
use herdr_agent_kit::agent_procs::fake::FakeLiveness;
use herdr_agent_kit::agent_procs::{Liveness, NoProcs, ProcessIdentity};
use herdr_agent_kit::claude_session::fold::{
    ACTIVE, ASYNC_QUIET, BG_CAP, MONITOR_SLACK, Owner, PENDING_CAP, REOPEN_MARGIN, Status,
};
use herdr_agent_kit::claude_session::{Scanner, SessionId};
use serde_json::{Map, Value, json};

const SID: &str = "55555555-5555-4555-8555-555555555555";
const M: f64 = 60.0;
const B: f64 = BASE as f64;

/// `t` as Claude Code writes it, with milliseconds.
fn iso(t: f64) -> String {
    let secs = t.floor() as i64;
    let ms = ((t - t.floor()) * 1000.0).round() as i64;
    common::iso(secs).replace(".000Z", &format!(".{ms:03}Z"))
}

fn notification(task: &str, call: &str, status: &str, tool_uses: u32) -> String {
    format!(
        "<task-notification>\n<task-id>{task}</task-id>\n<tool-use-id>{call}</tool-use-id>\n\
         <output-file>/tmp/tasks/{task}.output</output-file>\n<status>{status}</status>\n\
         <summary>Agent finished</summary>\n<result>report</result>\n\
         <usage><subagent_tokens>1000</subagent_tokens><tool_uses>{tool_uses}</tool_uses>\
         <duration_ms>1000</duration_ms></usage>\n</task-notification>"
    )
}

fn tool_block(
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

fn result_block(
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

fn tur(rec: &mut Map<String, Value>, fields: Value) {
    let t = rec["toolUseResult"]
        .as_object_mut()
        .expect("a toolUseResult");
    for (k, v) in fields.as_object().expect("fields") {
        t.insert(k.clone(), v.clone());
    }
}

/// One transcript, written from templates (scenario.Transcript).
struct Tx {
    path: PathBuf,
    agent: Option<String>,
    lines: Vec<String>,
    n: usize,
}

impl Tx {
    fn kind(&self, main: &str, sub: &str) -> Map<String, Value> {
        tpl(if self.agent.is_some() { sub } else { main })
    }

    fn tid(&mut self) -> String {
        self.n += 1;
        format!(
            "toolu_{}_{}",
            self.agent.as_deref().unwrap_or("main"),
            self.n
        )
    }

    fn add(&mut self, mut rec: Map<String, Value>, t: f64) {
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

    fn prompt(&mut self, t: f64, text: &str) {
        let mut rec = self.kind("user_prompt_human", "sub_first_user");
        rec["message"]["content"] = json!(text);
        self.add(rec, t);
    }

    fn text(&mut self, t: f64, text: &str) {
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
    fn bash(&mut self, t: f64, description: &str, pending: bool) {
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

    fn handback(&mut self, t: f64) {
        let id = self.tid();
        let rec = tpl("sub_assistant_handback");
        self.add(
            tool_block(rec, "SubagentHandback", &id, json!({"message": "done"})),
            t,
        );
    }

    /// An Agent call; a background one gets its `async_launched` result.
    fn launch(&mut self, t: f64, agent: &str, description: &str, background: bool) -> String {
        self.launch_as(t, agent, description, background, "general-purpose")
    }

    /// [`Tx::launch`] of a given `subagent_type`.
    fn launch_as(
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

    fn foreground_done(&mut self, t: f64, call: &str, agent: &str, error: bool) {
        let mut res = tpl("result_agent_foreground");
        tur(
            &mut res,
            json!({"agentId": agent, "status": "completed", "totalToolUseCount": 3}),
        );
        self.add(result_block(res, call, "report", Some(error)), t);
    }

    /// A `<task-notification>` in the given carriers (`queue`, `attachment`, `user`).
    fn notify(&mut self, t: f64, task: &str, call: &str, status: &str, carriers: &[&str]) {
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

    fn resume(&mut self, t: f64, agent: &str) {
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

    fn bash_background(&mut self, t: f64, task: &str) -> String {
        let id = self.tid();
        let rec = tpl("assistant_bash_background");
        let input =
            json!({"command": "sleep 60", "description": "watch", "run_in_background": true});
        self.add(tool_block(rec, "Bash", &id, input), t);
        let mut res = self.kind("result_bash_background", "sub_result_bash_background");
        res["toolUseResult"]["backgroundTaskId"] = json!(task);
        self.add(
            result_block(
                res,
                &id,
                &format!("Command running in background with ID: {task}."),
                None,
            ),
            t + 1.0,
        );
        id
    }

    fn monitor(&mut self, t: f64, task: &str, timeout_ms: u64, persistent: bool) {
        let id = self.tid();
        let input = json!({"description": "watch", "timeout_ms": timeout_ms, "command": "true"});
        self.add(
            tool_block(tpl("assistant_monitor"), "Monitor", &id, input),
            t,
        );
        let mut res = tpl("result_monitor");
        tur(
            &mut res,
            json!({"taskId": task, "timeoutMs": timeout_ms, "persistent": persistent}),
        );
        self.add(
            result_block(res, &id, &format!("Monitor started (task {task})."), None),
            t + 1.0,
        );
    }

    fn task_stop(&mut self, t: f64, task: &str, error: bool) {
        let id = self.tid();
        self.add(
            tool_block(
                tpl("assistant_taskstop"),
                "TaskStop",
                &id,
                json!({"task_id": task}),
            ),
            t,
        );
        let mut res = tpl("result_taskstop");
        let msg = format!("Successfully stopped task: {task}");
        tur(
            &mut res,
            json!({"message": msg, "task_id": task, "task_type": "local_bash"}),
        );
        self.add(result_block(res, &id, &msg, Some(error)), t + 1.0);
    }

    fn monitor_expired(&mut self, t: f64, task: &str) {
        let mut q = tpl("queue_enqueue_monitor_expired");
        q["content"] = json!(format!(
            "<task-notification>\n<task-id>{task}</task-id>\n<summary>Monitor event</summary>\n\
             <event>[Monitor expired after 30m]</event>\n</task-notification>"
        ));
        self.add(q, t);
    }

    fn workflow(&mut self, t: f64, run: &str, task: &str) {
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

/// A session under construction: its main transcript, agents, journals,
/// run summaries and hook records.
struct Sc {
    root: PathBuf,
    main: Tx,
    agents: BTreeMap<String, (Tx, Map<String, Value>)>,
    journals: BTreeMap<String, Vec<String>>,
    summaries: BTreeMap<String, Value>,
    hooks: Vec<String>,
}

impl Sc {
    fn new(name: &str) -> Sc {
        let root = tmp_dir(&format!("fold-{name}"));
        let main = Tx {
            path: root.join(SLUG).join(format!("{SID}.jsonl")),
            agent: None,
            lines: Vec::new(),
            n: 0,
        };
        Sc {
            root,
            main,
            agents: BTreeMap::new(),
            journals: BTreeMap::new(),
            summaries: BTreeMap::new(),
            hooks: Vec::new(),
        }
    }

    fn session_dir(&self) -> PathBuf {
        self.root.join(SLUG).join(SID)
    }

    /// An agent's transcript and meta (scenario.Project.agent).
    fn agent(&mut self, id: &str, meta: Value, kind: &str, run: Option<&str>) -> &mut Tx {
        let mut m = tpl(kind);
        let given = meta.as_object().expect("meta");
        for key in m.keys().cloned().collect::<Vec<_>>() {
            if let Some(v) = given.get(&key) {
                m.insert(key, v.clone());
            }
        }
        for key in ["parentAgentId", "workflowPhase", "toolUseId", "spawnDepth"] {
            match given.get(key) {
                Some(v) => {
                    m.insert(key.into(), v.clone());
                }
                None if key == "parentAgentId" || key == "toolUseId" => {
                    m.remove(key);
                }
                None => {}
            }
        }
        let mut dir = self.session_dir().join("subagents");
        if let Some(run) = run {
            dir = dir.join("workflows").join(run);
        }
        let tx = Tx {
            path: dir.join(format!("agent-{id}.jsonl")),
            agent: Some(id.into()),
            lines: Vec::new(),
            n: 0,
        };
        self.agents.insert(id.into(), (tx, m));
        &mut self.agents.get_mut(id).expect("just added").0
    }

    fn journal(&mut self, run: &str, kind: &str, agent: &str, phase: &str) {
        let mut rec = tpl(&format!("journal_{kind}"));
        for (k, v) in [("agentId", agent), ("label", agent), ("phase", phase)] {
            if rec.contains_key(k) {
                rec.insert(k.into(), json!(v));
            }
        }
        self.journals
            .entry(run.into())
            .or_default()
            .push(serde_json::to_string(&rec).expect("json"));
    }

    fn summary(&mut self, run: &str, status: &str) {
        let mut rec = tpl("workflow_summary");
        rec.insert("runId".into(), json!(run));
        rec.insert("status".into(), json!(status));
        self.summaries.insert(run.into(), Value::Object(rec));
    }

    /// A panorama hook record (hooks.py `record`, with the process `_append` adds).
    fn hook(&mut self, event: &str, agent: &str, ts: f64, pid: u32, start: &str) {
        let rec = json!({"v": 1, "event": event, "agent": agent, "agent_type": "general-purpose",
                         "ts": ts, "pid": pid, "pstart": start, "pidns": "pid:[1]"});
        self.hooks.push(rec.to_string());
    }

    fn write(path: &PathBuf, lines: &[String]) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        let mut body = lines.join("\n");
        body.push('\n');
        std::fs::write(path, body).expect("write");
    }

    /// Writes every file, then reads them all with a new [`Scanner`].
    fn load(&self) -> Scanner {
        let mut s = self.write_all();
        s.update(None);
        s
    }

    /// Writes every file; the returned [`Scanner`] has read nothing yet.
    fn write_all(&self) -> Scanner {
        Sc::write(&self.main.path, &self.main.lines);
        for (tx, meta) in self.agents.values() {
            Sc::write(&tx.path, &tx.lines);
            let meta_path = tx.path.with_file_name(format!(
                "agent-{}.meta.json",
                tx.agent.as_deref().expect("an agent")
            ));
            std::fs::write(meta_path, serde_json::to_string(meta).expect("json"))
                .expect("write the meta");
        }
        for (run, lines) in &self.journals {
            Sc::write(
                &self
                    .session_dir()
                    .join("subagents/workflows")
                    .join(run)
                    .join("journal.jsonl"),
                lines,
            );
        }
        for (run, summary) in &self.summaries {
            let path = self
                .session_dir()
                .join("workflows")
                .join(format!("{run}.json"));
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
            std::fs::write(path, summary.to_string()).expect("write the summary");
        }
        let hooks = self.root.join("hooks").join(format!("{SID}.jsonl"));
        if !self.hooks.is_empty() {
            Sc::write(&hooks, &self.hooks);
        }
        Scanner::new(
            SessionId::parse(SID).expect("an id"),
            self.main.path.clone(),
            Some(CWD.into()),
            Some(hooks),
        )
    }

    fn tx(&mut self, id: &str) -> &mut Tx {
        &mut self.agents.get_mut(id).expect("an agent").0
    }
}

fn bg_agent(sc: &mut Sc, id: &str, desc: &str) -> String {
    let call = sc.main.launch(B, id, desc, true);
    let a = sc.agent(
        id,
        json!({"description": desc, "toolUseId": call}),
        "meta_background",
        None,
    );
    a.prompt(B, "brief");
    call
}

/// One row: how to build the session, the agent asked about, the instant,
/// the process checks, and the expected `(status, inferred)`.
struct Row {
    rule: &'static str,
    /// The agent asked about; `None`: main's own background work.
    agent: Option<&'static str>,
    build: fn(&mut Sc) -> f64,
    live: fn() -> FakeLiveness,
    want: (Status, bool),
}

fn unknown() -> FakeLiveness {
    FakeLiveness::default()
}

fn no_tasks() -> FakeLiveness {
    FakeLiveness::default().tasks::<&str>(&[])
}

fn task_b1_running() -> FakeLiveness {
    FakeLiveness::default().tasks(&["b1"])
}

fn hook_process_gone() -> FakeLiveness {
    FakeLiveness::default().process(
        ProcessIdentity::new(4242, "s1", Some("pid:[1]".into())),
        false,
    )
}

fn hook_process_alive() -> FakeLiveness {
    FakeLiveness::default().process(
        ProcessIdentity::new(4242, "s1", Some("pid:[1]".into())),
        true,
    )
}

const ROWS: &[Row] = &[
    // -- 1. terminal evidence --------------------------------------------------------------
    Row {
        rule: "1: a completed notice from the queue carrier alone ends the agent",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + 2.0 * M, "look", false);
            sc.main
                .notify(B + 3.0 * M, "a1", &call, "completed", &["queue"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a notice from the attachment carrier counts",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 3.0 * M, "a1", &call, "completed", &["attachment"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a notice in a plain user string counts (older sessions)",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 3.0 * M, "a1", &call, "completed", &["user"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a notice in the spawning agent's own file counts",
        agent: Some("c1"),
        build: |sc| {
            bg_agent(sc, "p1", "parent");
            let tx = &mut sc.agents.get_mut("p1").unwrap().0;
            let call = tx.launch(B + M, "c1", "child", true);
            let tx = sc.agent("c1", json!({"description": "child", "toolUseId": call, "parentAgentId": "p1", "spawnDepth": 2}), "meta_nested", None);
            tx.prompt(B + M, "brief");
            sc.agents.get_mut("p1").unwrap().0.notify(
                B + 3.0 * M,
                "c1",
                &call,
                "completed",
                &["attachment"],
            );
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: failed is fail",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "failed", &["queue", "attachment"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Failed, false),
    },
    Row {
        rule: "1: killed is cxl",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.agents.get_mut("a1").unwrap().0.bash(B + M, "work", true);
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "killed", &["queue", "attachment"]);
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: stopped is cxl",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "stopped", &["queue"]);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: the latest notice wins (failed, resumed, completed: done)",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "failed", &["queue"]);
            sc.main.resume(B + 3.0 * M, "a1");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + 4.0 * M, "retry", false);
            sc.main
                .notify(B + 5.0 * M, "a1", &call, "completed", &["queue"]);
            B + 6.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a foreground spawn's completed acknowledgement ends it",
        agent: Some("f1"),
        build: |sc| {
            let call = sc.main.launch(B, "f1", "fg", false);
            sc.agent(
                "f1",
                json!({"description": "fg", "toolUseId": call}),
                "meta_background",
                None,
            )
            .prompt(B, "brief");
            sc.main.foreground_done(B + 2.0 * M, &call, "f1", false);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: an erroring foreground acknowledgement is fail",
        agent: Some("f1"),
        build: |sc| {
            let call = sc.main.launch(B, "f1", "fg", false);
            sc.agent(
                "f1",
                json!({"description": "fg", "toolUseId": call}),
                "meta_background",
                None,
            )
            .prompt(B, "brief");
            sc.main.foreground_done(B + 2.0 * M, &call, "f1", true);
            B + 30.0 * M
        },
        live: unknown,
        want: (Status::Failed, false),
    },
    Row {
        rule: "1: a workflow journal result is done",
        agent: Some("w1"),
        build: |sc| {
            sc.main.workflow(B, "wf_1", "t_wf");
            sc.agent(
                "w1",
                json!({"description": "step", "workflowPhase": "build"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", false);
            sc.journal("wf_1", "started", "w1", "build");
            sc.journal("wf_1", "result", "w1", "build");
            B + 2.0 * M
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "1: a workflow journal failure is fail",
        agent: Some("w1"),
        build: |sc| {
            sc.main.workflow(B, "wf_1", "t_wf");
            sc.agent(
                "w1",
                json!({"description": "step"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", false);
            sc.journal("wf_1", "failed", "w1", "build");
            B + 2.0 * M
        },
        live: unknown,
        want: (Status::Failed, false),
    },
    Row {
        rule: "1: the end of the agent's killed workflow run is cxl",
        agent: Some("w1"),
        build: |sc| {
            sc.main.workflow(B, "wf_1", "t_wf");
            sc.agent(
                "w1",
                json!({"description": "step"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", true);
            sc.main
                .notify(B + 2.0 * M, "t_wf", "toolu_wf", "killed", &["queue"]);
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    Row {
        rule: "1: a killed run whose launch was compacted away: its summary ends it (cxl)",
        agent: Some("w1"),
        build: |sc| {
            sc.agent(
                "w1",
                json!({"description": "step"}),
                "meta_workflow",
                Some("wf_1"),
            )
            .bash(B + M, "build", false);
            sc.summary("wf_1", "killed");
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Cancelled, false),
    },
    // -- 2. evidence overturned ------------------------------------------------------------
    Row {
        rule: "2: a later SendMessage resume reopens a finished agent",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.main.resume(B + 3.0 * M, "a1");
            B + 4.0 * M
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "2: a resumed agent quiet for less than ASYNC_QUIET is still working (inferred)",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.main.resume(B + 3.0 * M, "a1");
            B + 3.0 * M + 1.0 + ACTIVE + 1.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
    Row {
        rule: "2: the agent's own activity more than REOPEN_MARGIN after its notice reopens it",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .text(B + 2.0 * M + REOPEN_MARGIN + 10.0, "more");
            B + 3.0 * M
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "2: a final line within REOPEN_MARGIN of the notice does not reopen it",
        agent: Some("a1"),
        build: |sc| {
            let call = bg_agent(sc, "a1", "x");
            sc.main
                .notify(B + 2.0 * M, "a1", &call, "completed", &["queue"]);
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .text(B + 2.0 * M + REOPEN_MARGIN - 2.0, "report");
            B + 2.0 * M + 30.0
        },
        live: unknown,
        want: (Status::Done, false),
    },
    // -- 3. no standing evidence: liveness --------------------------------------------------
    Row {
        rule: "3: written within ACTIVE is running",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + 10.0 * M, "work", false);
            B + 10.0 * M + 1.0 + ACTIVE - 1.0
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "3: a tool call in flight keeps it running up to PENDING_CAP",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents.get_mut("a1").unwrap().0.bash(B + M, "long", true);
            B + M + PENDING_CAP - 1.0
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "3: an abandoned call past PENDING_CAP no longer keeps it running",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents.get_mut("a1").unwrap().0.bash(B + M, "long", true);
            B + M + PENDING_CAP + 1.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: a background agent not notified yet is running within ASYNC_QUIET (inferred)",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + M, "work", false);
            B + M + 1.0 + ASYNC_QUIET - 1.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
    Row {
        rule: "3: past ASYNC_QUIET a silent background agent ended silently (done, inferred)",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash(B + M, "work", false);
            B + M + 1.0 + ASYNC_QUIET + 1.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: a foreground agent whose spawning call has no result yet is running",
        agent: Some("f1"),
        build: |sc| {
            let call = sc.main.launch(B, "f1", "fg", false);
            sc.agent(
                "f1",
                json!({"description": "fg", "toolUseId": call, "requestShape": "foreground"}),
                "meta_background",
                None,
            )
            .bash(B + M, "work", false);
            B + 40.0 * M
        },
        live: unknown,
        want: (Status::Run, false),
    },
    Row {
        rule: "3: a handback ends the agent (recorded, not inferred)",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents.get_mut("a1").unwrap().0.handback(B + M);
            B + M + PENDING_CAP + 10.0 // the handback call itself never gets a result
        },
        live: unknown,
        want: (Status::Done, false),
    },
    Row {
        rule: "3: an idle fork is done (inferred)",
        agent: Some("k1"),
        build: |sc| {
            let call = sc.main.launch(B, "k1", "fork", true);
            sc.agent(
                "k1",
                json!({"description": "fork", "toolUseId": call}),
                "meta_fork",
                None,
            )
            .bash(B + M, "work", false);
            B + M + ACTIVE + 10.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: an agent with no records yet, launched within ASYNC_QUIET, is running (inferred)",
        agent: Some("a1"),
        build: |sc| {
            let call = sc.main.launch(B, "a1", "x", true);
            sc.agent(
                "a1",
                json!({"description": "x", "toolUseId": call}),
                "meta_background",
                None,
            );
            B + ASYNC_QUIET - 10.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
    Row {
        rule: "3: an agent with no records, launched long ago, ended silently",
        agent: Some("a1"),
        build: |sc| {
            let call = sc.main.launch(B, "a1", "x", true);
            sc.agent(
                "a1",
                json!({"description": "x", "toolUseId": call}),
                "meta_background",
                None,
            );
            B + ASYNC_QUIET + 10.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "3: a silent agent with a running child is running (waiting on it)",
        agent: Some("p1"),
        build: |sc| {
            bg_agent(sc, "p1", "parent");
            let tx = &mut sc.agents.get_mut("p1").unwrap().0;
            let call = tx.launch(B + M, "c1", "child", true);
            let c = sc.agent("c1", json!({"description": "child", "toolUseId": call, "parentAgentId": "p1", "spawnDepth": 2}), "meta_nested", None);
            c.bash(B + 60.0 * M, "work", false);
            B + 60.0 * M + 10.0
        },
        live: unknown,
        want: (Status::Run, true),
    },
    // -- background work -------------------------------------------------------------------
    Row {
        rule: "bg: a turn that ended with its background Bash still running is waiting",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash_background(B + M, "b1");
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: a background Bash whose process is gone does not count",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash_background(B + M, "b1");
            B + 30.0 * M
        },
        live: no_tasks,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: an unchecked task counts up to BG_CAP after it started",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash_background(B + M, "b1");
            B + M + 1.0 + BG_CAP - 10.0
        },
        live: unknown,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: an unchecked task past BG_CAP no longer counts",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.agents
                .get_mut("a1")
                .unwrap()
                .0
                .bash_background(B + M, "b1");
            B + M + 1.0 + BG_CAP + 10.0
        },
        live: unknown,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a terminal notice naming the task ends it",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            let tx = &mut sc.agents.get_mut("a1").unwrap().0;
            let call = tx.bash_background(B + M, "b1");
            tx.notify(B + 2.0 * M, "b1", &call, "completed", &["attachment"]);
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a successful TaskStop ends it",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            let tx = &mut sc.agents.get_mut("a1").unwrap().0;
            tx.bash_background(B + M, "b1");
            tx.task_stop(B + 2.0 * M, "b1", false);
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a failed TaskStop does not end it",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            let tx = &mut sc.agents.get_mut("a1").unwrap().0;
            tx.bash_background(B + M, "b1");
            tx.task_stop(B + 2.0 * M, "b1", true);
            B + 30.0 * M
        },
        live: task_b1_running,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: a Monitor runs until its timeout plus MONITOR_SLACK",
        agent: None,
        build: |sc| {
            sc.main.monitor(B, "m1", 600_000, false);
            B + 1.0 + 600.0 + MONITOR_SLACK - 1.0
        },
        live: no_tasks,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: a Monitor past its timeout has expired",
        agent: None,
        build: |sc| {
            sc.main.monitor(B, "m1", 600_000, false);
            B + 1.0 + 600.0 + MONITOR_SLACK + 1.0
        },
        live: no_tasks,
        want: (Status::Done, true),
    },
    Row {
        rule: "bg: a persistent Monitor never times out",
        agent: None,
        build: |sc| {
            sc.main.monitor(B, "m1", 600_000, true);
            B + 3.0 * 3600.0
        },
        live: no_tasks,
        want: (Status::Bg, false),
    },
    Row {
        rule: "bg: a Monitor's [Monitor expired event ends it",
        agent: None,
        build: |sc| {
            sc.main.monitor(B, "m1", 600_000, false);
            sc.main.monitor_expired(B + 2.0 * M, "m1");
            B + 3.0 * M
        },
        live: no_tasks,
        want: (Status::Done, true),
    },
    // -- hooks -----------------------------------------------------------------------------
    Row {
        rule: "hooks: a recorded stop is an ending",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            sc.hook("stop", "a1", B + 2.0 * M, 4242, "s1");
            B + 3.0 * M
        },
        live: hook_process_alive,
        want: (Status::Done, false),
    },
    Row {
        rule: "hooks: started, never stopped, its Claude process gone: lost",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            B + 30.0 * M
        },
        live: hook_process_gone,
        want: (Status::Lost, false),
    },
    Row {
        rule: "hooks: started, never stopped, its process still alive: not lost",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            B + 30.0 * M
        },
        live: hook_process_alive,
        want: (Status::Done, true),
    },
    Row {
        rule: "hooks: a later record from another Claude process (a resume): not lost",
        agent: Some("a1"),
        build: |sc| {
            bg_agent(sc, "a1", "x");
            sc.hook("start", "a1", B, 4242, "s1");
            sc.hook("start", "other", B + M, 5555, "s9");
            B + 30.0 * M
        },
        live: hook_process_gone,
        want: (Status::Done, true),
    },
];

#[test]
fn the_status_rules() {
    let mut failures = Vec::new();
    for (i, row) in ROWS.iter().enumerate() {
        let mut sc = Sc::new(&format!("row{i}"));
        let now = (row.build)(&mut sc);
        let scanner = sc.load();
        let live = (row.live)();
        let got = match row.agent {
            Some(agent) => {
                let s = scanner.session().status_of(agent, now, &live);
                (s.status, s.inferred)
            }
            // no agent: the row is about main's own background work
            None => {
                let waiting = !scanner
                    .session()
                    .waiting_bg(&Owner::Main, now, &live)
                    .is_empty();
                (if waiting { Status::Bg } else { Status::Done }, !waiting)
            }
        };
        if got != row.want {
            failures.push(format!("{}: got {got:?}, want {:?}", row.rule, row.want));
        }
        let _ = std::fs::remove_dir_all(&sc.root);
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn a_resume_is_counted_and_restarts_the_clock() {
    let mut sc = Sc::new("resume-count");
    let call = bg_agent(&mut sc, "a1", "x");
    sc.main
        .notify(B + 2.0 * M, "a1", &call, "failed", &["queue"]);
    sc.main.resume(B + 3.0 * M, "a1");
    let s = sc.load();
    assert_eq!(s.session().resumes("a1").len(), 1);
    let st = s.session().status_of("a1", B + 3.0 * M + 30.0, &NoProcs);
    assert_eq!((st.status, st.ended), (Status::Run, None));
}

#[test]
fn ended_at_is_the_evidence_time() {
    let mut sc = Sc::new("ended-at");
    let call = bg_agent(&mut sc, "a1", "x");
    sc.main
        .notify(B + 3.0 * M, "a1", &call, "completed", &["queue"]);
    let s = sc.load();
    let st = s.session().status_of("a1", B + 30.0 * M, &NoProcs);
    assert_eq!(st.ended, Some(B + 3.0 * M));
}

#[test]
fn parentage_follows_the_spawning_call() {
    let mut sc = Sc::new("parentage");
    bg_agent(&mut sc, "p1", "parent");
    let call = sc
        .agents
        .get_mut("p1")
        .unwrap()
        .0
        .launch(B + M, "c1", "child", true);
    sc.agent(
        "c1",
        json!({"description": "child", "toolUseId": call, "spawnDepth": 2}),
        "meta_nested",
        None,
    )
    .prompt(B + M, "brief");
    // a meta that names the parent, whose spawning call was never read
    sc.agent(
        "o1",
        json!({"description": "orphan", "toolUseId": "toolu_gone", "parentAgentId": "p1"}),
        "meta_nested",
        None,
    )
    .prompt(B + M, "brief");
    let s = sc.load();
    let sess = s.session();
    assert_eq!(sess.parent_of("p1"), Some(Owner::Main));
    assert_eq!(sess.parent_of("c1"), Some(Owner::Agent("p1".into())));
    assert_eq!(sess.parent_of("o1"), Some(Owner::Agent("p1".into())));
    assert_eq!(sess.children(&Owner::Agent("p1".into())), vec!["c1", "o1"]);
}

#[test]
fn a_fork_repeating_its_parents_history_never_takes_the_spawning_call() {
    let mut sc = Sc::new("fork-history");
    let call = bg_agent(&mut sc, "a1", "x");
    // the fork's file repeats main's history, the spawning call included, later
    let tx = sc.agent(
        "k1",
        json!({"description": "fork", "toolUseId": "toolu_k"}),
        "meta_fork",
        None,
    );
    let rec = tool_block(
        tpl("sub_assistant_agent"),
        "Agent",
        &call,
        json!({"description": "x", "run_in_background": true}),
    );
    tx.add(rec, B + 5.0 * M);
    let s = sc.load();
    assert_eq!(
        s.session().spawn_of("a1").map(|sp| sp.owner.clone()),
        Some(Owner::Main)
    );
    assert_eq!(s.session().parent_of("a1"), Some(Owner::Main));
}

#[test]
fn the_fold_does_not_depend_on_the_order_files_are_read() {
    // the same records applied in two file orders give the same statuses
    let mut sc = Sc::new("order");
    let call = bg_agent(&mut sc, "a1", "x");
    sc.agents
        .get_mut("a1")
        .unwrap()
        .0
        .bash(B + M, "work", false);
    sc.main.notify(
        B + 2.0 * M,
        "a1",
        &call,
        "completed",
        &["queue", "attachment"],
    );
    let s = sc.load();
    let lines_main = std::fs::read(&sc.main.path).unwrap();
    let lines_a1 = std::fs::read(&sc.agents["a1"].0.path).unwrap();
    let split = |b: &[u8]| -> Vec<Vec<u8>> {
        b.split(|c| *c == b'\n')
            .filter(|l| !l.is_empty())
            .map(<[u8]>::to_vec)
            .collect()
    };
    let mut pure = herdr_agent_kit::claude_session::Session::new(SessionId::parse(SID).unwrap());
    pure.set_agent("a1", sc.agents["a1"].0.path.clone(), None);
    pure.set_meta("a1", sc.agents["a1"].1.clone());
    for (owner, lines) in [
        (Owner::Agent("a1".into()), split(&lines_a1)),
        (Owner::Main, split(&lines_main)),
    ] {
        for raw in &lines {
            let line = herdr_agent_kit::claude_session::Line::parse(raw).unwrap();
            let when = line.ts();
            let facts = pure.file_mut(owner.clone());
            if let Some(t) = when {
                facts.last_ts = Some(t.secs());
            }
            pure.apply(owner.clone(), &line, when);
        }
    }
    let want = s.session().status_of("a1", B + 30.0 * M, &NoProcs);
    let got = pure.status_of("a1", B + 30.0 * M, &NoProcs);
    assert_eq!(
        (got.status, got.ended, got.inferred),
        (want.status, want.ended, want.inferred)
    );
}

#[test]
fn liveness_is_only_asked_about_what_the_rules_need() {
    struct Panics;
    impl Liveness for Panics {
        fn process_alive(&self, _: &ProcessIdentity) -> Option<bool> {
            panic!("no hook records: no process check")
        }
        fn task_alive(&self, _: &str) -> Option<bool> {
            panic!("no background work: no task check")
        }
    }
    let mut sc = Sc::new("no-checks");
    bg_agent(&mut sc, "a1", "x");
    sc.agents
        .get_mut("a1")
        .unwrap()
        .0
        .bash(B + M, "work", false);
    let s = sc.load();
    assert_eq!(
        s.session().status_of("a1", B + 10.0 * M, &Panics).status,
        Status::Run
    );
}

#[test]
fn ask_operator_results_record_the_askers_question_ids() {
    let mut sc = Sc::new("qids");
    bg_agent(&mut sc, "a1", "x");
    let tx = &mut sc.agents.get_mut("a1").unwrap().0;
    let id = tx.tid();
    let name = "mcp__plugin_herdr-inbox_herdr-inbox__ask_operator";
    tx.add(
        tool_block(
            tpl("assistant_ask_operator"),
            name,
            &id,
            json!({"questions": [{"question": "q"}]}),
        ),
        B + M,
    );
    let text = json!({"group": 7, "questions": [{"id": 7, "status": "open", "message": "Recorded as Q7. Continue."}]}).to_string();
    tx.add(
        result_block(tpl("result_ask_operator"), &id, &text, None),
        B + M + 1.0,
    );
    let s = sc.load();
    let ids: Vec<u64> = s
        .session()
        .qids(&Owner::Agent("a1".into()))
        .into_iter()
        .flatten()
        .copied()
        .collect();
    assert_eq!(ids, vec![7]);
}

// -- updates after updates (panorama `test_scan_session.py` / `test_background.py`) ----------

fn set_mtime(path: &Path, t: f64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(UNIX_EPOCH + Duration::from_secs_f64(t))
        .expect("set the mtime");
}

/// The newest time read from agent `id`'s file: `None` while it is unread.
fn read_ts(s: &Scanner, id: &str) -> Option<f64> {
    s.session().last_ts(id)
}

/// panorama `test_incremental_reads_partial_lines_and_rewrites`.
#[test]
fn an_update_reads_partial_lines_and_a_rewrite_rebuilds() {
    let mut sc = Sc::new("incremental");
    let call = bg_agent(&mut sc, "a1", "x");
    let mut s = sc.load();
    let status = |s: &Scanner| s.session().status_of("a1", B + 4.0 * M, &NoProcs).status;
    assert_eq!(status(&s), Status::Run);
    let old = std::fs::read_to_string(&sc.main.path).unwrap();
    sc.main
        .notify(B + 3.0 * M, "a1", &call, "completed", &["queue"]);
    let full = sc.main.lines.join("\n") + "\n";
    std::fs::write(&sc.main.path, &full[..old.len() + 40]).unwrap(); // the notice is half written
    s.update(None);
    assert_eq!(status(&s), Status::Run);
    std::fs::write(&sc.main.path, &full).unwrap();
    s.update(None);
    assert_eq!(status(&s), Status::Done);
    assert_eq!(s.session().notices("a1").len(), 1);
    std::fs::write(&sc.main.path, &old).unwrap(); // rewritten shorter: rebuilt from the new content
    s.update(None);
    assert!(s.session().notices("a1").is_empty());
    assert_eq!(status(&s), Status::Run);
}

/// panorama `test_old_agent_files_are_skipped_until_they_change`.
#[test]
fn an_old_agent_file_is_skipped_until_it_changes() {
    let mut sc = Sc::new("skip-old");
    bg_agent(&mut sc, "a1", "x");
    let mut s = sc.write_all();
    let path = sc.agents["a1"].0.path.clone();
    set_mtime(&path, B);
    s.update(Some(B + 3600.0));
    assert_eq!(read_ts(&s, "a1"), None); // not read
    sc.tx("a1").bash(B + 7200.0, "again", false);
    Sc::write(&path, &sc.agents["a1"].0.lines);
    s.update(Some(B + 3600.0));
    assert_eq!(read_ts(&s, "a1"), Some(B + 7201.0));
}

/// A file once read is followed whatever its mtime says (the `since` skip is
/// for files never read).
#[test]
fn a_file_once_read_is_followed_even_with_an_old_mtime() {
    let mut sc = Sc::new("followed");
    bg_agent(&mut sc, "a1", "x");
    let mut s = sc.write_all();
    s.update(Some(B + 3600.0)); // written now: read
    assert_eq!(read_ts(&s, "a1"), Some(B));
    sc.tx("a1").bash(B + 2.0 * M, "more", false);
    let path = sc.agents["a1"].0.path.clone();
    Sc::write(&path, &sc.agents["a1"].0.lines);
    set_mtime(&path, B); // appended, but its mtime is old (a restored or copied file)
    s.update(Some(B + 3600.0));
    assert_eq!(read_ts(&s, "a1"), Some(B + 2.0 * M + 1.0));
}

/// panorama `test_background.py::test_an_old_agent_file_is_still_read_when_its_agent_never_ended`.
#[test]
fn an_old_agent_file_is_still_read_when_its_agent_never_ended() {
    let mut sc = Sc::new("old-bg");
    bg_agent(&mut sc, "a4", "x");
    sc.tx("a4").bash_background(B + M, "ab4");
    let done = bg_agent(&mut sc, "a5", "y");
    sc.main
        .notify(B + 2.0 * M, "a5", &done, "completed", &["queue"]);
    let mut s = sc.write_all();
    for id in ["a4", "a5"] {
        set_mtime(&sc.agents[id].0.path, B + 2.0 * M);
    }
    s.update(Some(B + 3.0 * 3600.0)); // both files are older than the window
    assert_eq!(
        s.session()
            .status_of("a4", B + 4.0 * 3600.0, &NoProcs)
            .status,
        Status::Bg
    );
    assert_eq!(read_ts(&s, "a5"), None); // a finished one is still skipped
    // read once per file version: a second update does not read it again
    s.update(Some(B + 3.0 * 3600.0));
    assert_eq!(read_ts(&s, "a5"), None);
}

/// panorama `test_l2_hook_data_survives_a_rebuild`.
#[test]
fn hook_records_survive_a_rebuild() {
    let mut sc = Sc::new("hooks-rebuild");
    bg_agent(&mut sc, "a1", "x");
    sc.hook("stop", "a1", B + 2.0 * M, 4242, "s1");
    let mut s = sc.load();
    let got = |s: &Scanner| {
        let st = s.session().status_of("a1", B + 3.0 * M, &NoProcs);
        (st.status, st.ended, st.inferred)
    };
    let want = (Status::Done, Some(B + 2.0 * M), false);
    assert_eq!(got(&s), want);
    let full = std::fs::read_to_string(&sc.main.path).unwrap();
    let first = &full[..=full.find('\n').unwrap()];
    std::fs::write(&sc.main.path, first).unwrap(); // rewritten shorter: a rebuild
    s.update(None);
    assert_eq!(got(&s), want);
    std::fs::write(&sc.main.path, &full).unwrap(); // and back
    s.update(None);
    assert_eq!(got(&s), want);
}

/// panorama `test_meta_read_mid_write_is_retried`.
#[test]
fn a_meta_read_mid_write_is_retried() {
    let mut sc = Sc::new("meta-retry");
    bg_agent(&mut sc, "a1", "x");
    let mut s = sc.write_all();
    let meta = sc.agents["a1"].0.path.with_file_name("agent-a1.meta.json");
    let good = std::fs::read_to_string(&meta).unwrap();
    std::fs::write(&meta, &good[..10]).unwrap();
    set_mtime(&meta, B);
    s.update(None);
    assert!(s.session().agents()["a1"].meta.is_empty());
    std::fs::write(&meta, &good).unwrap();
    set_mtime(&meta, B + 1.0);
    s.update(None);
    assert_eq!(s.session().agents()["a1"].meta["description"], "x");
}

/// `meta.json` and run summaries are read as Python's `json.load` reads
/// them: a lone UTF-16 surrogate escape is accepted, a UTF-8 BOM is not.
#[test]
fn metas_and_summaries_take_a_lone_surrogate_like_python() {
    let mut sc = Sc::new("meta-surrogate");
    let call = bg_agent(&mut sc, "a1", "x");
    let tx = sc.agent(
        "b1",
        json!({"description": "y", "toolUseId": "toolu_b"}),
        "meta_background",
        None,
    );
    tx.prompt(B, "brief");
    sc.main.workflow(B, "wf_1", "task1");
    sc.journal("wf_1", "started", "w1", "Build");
    let mut s = sc.write_all();
    let meta = sc.agents["a1"].0.path.with_file_name("agent-a1.meta.json");
    std::fs::write(
        &meta,
        format!(r#"{{"description": "cut \ud83d", "toolUseId": "{call}"}}"#),
    )
    .unwrap();
    let bom = sc.agents["b1"].0.path.with_file_name("agent-b1.meta.json");
    std::fs::write(&bom, b"\xef\xbb\xbf{\"description\": \"y\"}").unwrap();
    let summary = sc.session_dir().join("workflows/wf_1.json");
    std::fs::create_dir_all(summary.parent().unwrap()).unwrap();
    std::fs::write(
        &summary,
        r#"{"runId": "wf_1", "status": "killed", "name": "a \udc00"}"#,
    )
    .unwrap();
    s.update(None);
    let sess = s.session();
    assert_eq!(sess.agents()["a1"].meta["description"], "cut \u{fffd}");
    assert_eq!(sess.parent_of("a1"), Some(Owner::Main));
    assert!(sess.agents()["b1"].meta.is_empty());
    assert_eq!(
        sess.wf_summary("wf_1").map(|m| m["status"].clone()),
        Some(json!("killed"))
    );
}

/// panorama `test_a_fork_does_not_take_over_its_own_spawning_call`.
#[test]
fn a_fork_does_not_take_over_its_own_spawning_call() {
    let mut sc = Sc::new("fork-own-call");
    let call = sc.main.launch_as(B, "k1", "fork it", true, "fork");
    let copied = sc.main.lines[0].clone();
    let k1 = sc.agent(
        "k1",
        json!({"description": "fork it", "agentType": "fork", "toolUseId": call}),
        "meta_fork",
        None,
    );
    k1.lines.push(copied); // the copied history
    k1.prompt(B + 1.0, "brief");
    let s = sc.load();
    assert_eq!(s.session().parent_of("k1"), Some(Owner::Main));
    assert!(s.session().children(&Owner::Agent("k1".into())).is_empty());
}

/// panorama `test_n1_a_fork_of_a_subagent_nests_under_it_even_when_only_its_own_copy_is_read`:
/// the fork's own file repeats its spawning call; the real parent's file is
/// skipped (`since`), so only the fork's own copy is read, and it never counts.
#[test]
fn a_fork_of_a_subagent_nests_under_it_when_only_its_own_copy_is_read() {
    let mut sc = Sc::new("fork-own-copy");
    bg_agent(&mut sc, "p1", "parent");
    let inner = sc.tx("p1").launch_as(B + M, "k1", "fork it", true, "fork");
    let copied = sc.tx("p1").lines[1].clone(); // the spawning call
    let k1 = sc.agent(
        "k1",
        json!({"description": "fork it", "agentType": "fork", "toolUseId": inner,
               "spawnDepth": 2, "parentAgentId": "p1"}),
        "meta_fork",
        None,
    );
    k1.lines.push(copied);
    k1.prompt(B + 2.0 * M, "brief");
    let mut s = sc.write_all();
    set_mtime(&sc.agents["p1"].0.path, B); // the parent's file is old: skipped
    s.update(Some(B + 60.0));
    assert_eq!(read_ts(&s, "p1"), None);
    let sess = s.session();
    assert_eq!(sess.parent_of("k1"), Some(Owner::Agent("p1".into())));
    assert_eq!(sess.children(&Owner::Agent("p1".into())), vec!["k1"]);
    assert!(sess.children(&Owner::Agent("k1".into())).is_empty());
}

/// Terminal evidence at one instant: Python's `max(evidence)` compares the
/// status strings, `cxl` < `done` < `fail`, whatever order they were read in.
#[test]
fn evidence_at_one_instant_ties_like_pythons_status_strings() {
    let cases = [
        (["killed", "completed"], Status::Done),
        (["completed", "failed"], Status::Failed),
        (["killed", "failed"], Status::Failed),
    ];
    for (i, (pair, want)) in cases.into_iter().enumerate() {
        for order in [[pair[0], pair[1]], [pair[1], pair[0]]] {
            let mut sc = Sc::new(&format!("tie{i}"));
            let call = bg_agent(&mut sc, "a1", "x");
            for status in order {
                sc.main.notify(B + 2.0 * M, "a1", &call, status, &["queue"]);
            }
            let s = sc.load();
            let st = s.session().status_of("a1", B + 3.0 * M, &NoProcs);
            assert_eq!(
                (st.status, st.ended),
                (want, Some(B + 2.0 * M)),
                "{order:?}"
            );
            let _ = std::fs::remove_dir_all(&sc.root);
        }
    }
}
