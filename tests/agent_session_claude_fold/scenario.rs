//! [`Sc`]: a session under construction (scenario.py).

use super::*;

/// A session under construction: its main transcript, agents, journals,
/// run summaries and hook records.
pub(crate) struct Sc {
    pub(crate) root: PathBuf,
    pub(crate) main: Tx,
    pub(crate) agents: BTreeMap<String, (Tx, Map<String, Value>)>,
    pub(crate) journals: BTreeMap<String, Vec<String>>,
    pub(crate) summaries: BTreeMap<String, Value>,
    pub(crate) hooks: Vec<String>,
}

impl Sc {
    pub(crate) fn new(name: &str) -> Sc {
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

    pub(crate) fn session_dir(&self) -> PathBuf {
        self.root.join(SLUG).join(SID)
    }

    /// An agent's transcript and meta (scenario.Project.agent).
    pub(crate) fn agent(
        &mut self,
        id: &str,
        meta: Value,
        kind: &str,
        run: Option<&str>,
    ) -> &mut Tx {
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

    pub(crate) fn journal(&mut self, run: &str, kind: &str, agent: &str, phase: &str) {
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

    pub(crate) fn summary(&mut self, run: &str, status: &str) {
        let mut rec = tpl("workflow_summary");
        rec.insert("runId".into(), json!(run));
        rec.insert("status".into(), json!(status));
        self.summaries.insert(run.into(), Value::Object(rec));
    }

    /// A panorama hook record (hooks.py `record`, with the process `_append` adds).
    pub(crate) fn hook(&mut self, event: &str, agent: &str, ts: f64, pid: u32, start: &str) {
        let rec = json!({"v": 1, "event": event, "agent": agent, "agent_type": "general-purpose",
                         "ts": ts, "pid": pid, "pstart": start, "pidns": "pid:[1]"});
        self.hooks.push(rec.to_string());
    }

    pub(crate) fn write(path: &PathBuf, lines: &[String]) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        let mut body = lines.join("\n");
        body.push('\n');
        std::fs::write(path, body).expect("write");
    }

    /// Writes every file, then reads them all with a new [`Scanner`].
    pub(crate) fn load(&self) -> Scanner {
        let mut s = self.write_all();
        s.update(None);
        s
    }

    /// Writes every file; the returned [`Scanner`] has read nothing yet.
    pub(crate) fn write_all(&self) -> Scanner {
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

    pub(crate) fn tx(&mut self, id: &str) -> &mut Tx {
        &mut self.agents.get_mut(id).expect("an agent").0
    }
}

pub(crate) fn bg_agent(sc: &mut Sc, id: &str, desc: &str) -> String {
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
