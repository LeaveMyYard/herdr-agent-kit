//! A session's subagent transcripts, workflow journals and run summaries.

use std::fs;
use std::path::{Path, PathBuf};

/// One subagent transcript of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AgentFile {
    /// The agent id (`agent-<id>.jsonl`).
    pub agent_id: String,
    /// Its transcript.
    pub transcript: PathBuf,
    /// Its `agent-<id>.meta.json` (it may not exist yet).
    pub meta: PathBuf,
    /// The workflow run it belongs to (`subagents/workflows/<run>/`).
    pub workflow: Option<String>,
}

fn session_dir(main_transcript: &Path) -> PathBuf {
    main_transcript.with_extension("")
}

/// `<session>/subagents/` next to a main transcript `<session>.jsonl`.
pub fn subagents_dir(main_transcript: &Path) -> PathBuf {
    session_dir(main_transcript).join("subagents")
}

/// The `agent-<id>.jsonl` files of a directory, by agent id. A missing
/// directory is empty (Claude Code creates them lazily). Names that are not
/// UTF-8 are skipped.
pub fn scan_agent_files(dir: &Path, workflow: Option<&str>) -> Vec<AgentFile> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<AgentFile> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let aid = name.strip_prefix("agent-")?.strip_suffix(".jsonl")?;
            (!aid.is_empty()).then(|| AgentFile {
                agent_id: aid.to_string(),
                transcript: e.path(),
                meta: dir.join(format!("agent-{aid}.meta.json")),
                workflow: workflow.map(str::to_string),
            })
        })
        .collect();
    out.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));
    out
}

/// The workflow run ids under `subagents/workflows/`, sorted.
pub fn scan_workflow_ids(subagents: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(subagents.join("workflows")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    out.sort();
    out
}

/// A session's subagent files and workflow journals (`{run: journal path}`):
/// panorama's `discovery.related`.
pub fn related(main_transcript: &Path) -> (Vec<AgentFile>, Vec<(String, PathBuf)>) {
    let subs = subagents_dir(main_transcript);
    let mut files = scan_agent_files(&subs, None);
    let mut journals = Vec::new();
    for run in scan_workflow_ids(&subs) {
        let dir = subs.join("workflows").join(&run);
        files.extend(scan_agent_files(&dir, Some(&run)));
        journals.push((run, dir.join("journal.jsonl")));
    }
    (files, journals)
}

/// `<session>/workflows/<run>.json`: a workflow run's summary.
pub fn workflow_summary_path(main_transcript: &Path, run: &str) -> PathBuf {
    session_dir(main_transcript)
        .join("workflows")
        .join(format!("{run}.json"))
}
