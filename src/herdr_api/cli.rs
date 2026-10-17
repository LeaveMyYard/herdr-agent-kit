//! The CLI path (`$HERDR_BIN_PATH`, else `herdr`), only for what herdr's
//! socket has no method for:
//!
//! - `herdr machine list --json` (panorama `machines.saved`, herdr-inbox
//!   `remote.machines`);
//! - `herdr --machine <id> api snapshot` (panorama `machines.remote_snapshot`,
//!   8 s) and `herdr --machine <id> agent focus <pane>` (panorama's remote
//!   focus);
//! - `herdr plugin config-dir <id>` for another plugin's id (herdr-inbox
//!   radar).
//!
//! Like the Python `run_raw`: every argument goes through [`clean`], the
//! default timeout is 3 s, and a failure is an error, never a panic.
//!
//! - The binary cannot be started: [`HerdrError::Unreachable`].
//! - Timed out (the process is killed): [`HerdrError::NoReply`].
//! - Exit status != 0: [`HerdrError::Refused`] with herdr's `{"error":
//!   {"code", "message"}}` from stderr, or with an empty code and the plain
//!   text (usage errors, `--machine <unknown>`: exit 2). The code
//!   `server_not_running` is [`HerdrError::NotRunning`] (herdr-inbox
//!   `server_gone`).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::error::{HerdrError, SERVER_NOT_RUNNING};
use super::ids::PaneId;
use super::sanitize::{MAX_ARG, clean};
use super::types::{Machine, Snapshot};
use super::wire;

/// The timeout of `--machine <id> api snapshot` (panorama
/// `machines.SNAPSHOT_TIMEOUT`).
pub const MACHINE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(8);

/// One CLI invocation: its arguments (after the binary) and its timeout.
pub(crate) struct Invocation {
    pub args: Vec<String>,
    pub timeout: Option<Duration>,
}

impl Invocation {
    fn new(args: &[&str]) -> Self {
        Self {
            args: args.iter().map(|a| clean(a, MAX_ARG)).collect(),
            timeout: None,
        }
    }
}

pub(crate) fn machine_list() -> Invocation {
    Invocation::new(&["machine", "list", "--json"])
}

pub(crate) fn machine_snapshot(machine: &str) -> Invocation {
    let mut inv = Invocation::new(&["--machine", machine, "api", "snapshot"]);
    inv.timeout = Some(MACHINE_SNAPSHOT_TIMEOUT);
    inv
}

pub(crate) fn machine_focus_agent(machine: &str, pane: &PaneId) -> Invocation {
    Invocation::new(&["--machine", machine, "agent", "focus", pane.as_str()])
}

pub(crate) fn plugin_config_dir(plugin_id: &str) -> Invocation {
    Invocation::new(&["plugin", "config-dir", plugin_id])
}

/// `machine list --json`: a list of machines, or (tolerated, as herdr-inbox
/// does) a `{"result": {"machines": [...]}}` envelope. Empty output is no
/// machines; output that is not JSON is [`HerdrError::Garbled`] (both Python
/// callers read that as no machines). Items that are not objects are
/// skipped; nothing is filtered (`enabled`, a safe `target`: the callers'
/// rules).
pub(crate) fn parse_machines(stdout: &str) -> Result<Vec<Machine>, HerdrError> {
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }
    let data: Value =
        serde_json::from_str(stdout).map_err(|e| HerdrError::Garbled(e.to_string()))?;
    let items = match data {
        Value::Array(items) => items,
        Value::Object(mut o) => match o
            .remove("result")
            .as_mut()
            .and_then(|r| r.get_mut("machines"))
            .map(Value::take)
        {
            Some(Value::Array(items)) => items,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    Ok(items
        .into_iter()
        .filter(Value::is_object)
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// `--machine <id> api snapshot`: `result.snapshot`, else Garbled.
pub(crate) fn parse_snapshot(stdout: &str) -> Result<Snapshot, HerdrError> {
    let data: Value = serde_json::from_str(stdout)
        .map_err(|_| HerdrError::Garbled("unreadable snapshot".into()))?;
    match data.pointer("/result/snapshot") {
        Some(snap @ Value::Object(_)) => serde_json::from_value(snap.clone())
            .map_err(|_| HerdrError::Garbled("unreadable snapshot".into())),
        _ => Err(HerdrError::Garbled("no snapshot".into())),
    }
}

/// `plugin config-dir`: the trimmed path, or `None` when herdr printed none.
pub(crate) fn parse_config_dir(stdout: &str) -> Option<PathBuf> {
    let dir = stdout.trim();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// A non-zero exit as an error (Python: `(stderr or stdout).strip() or
/// f"exit {code}"`).
fn exit_error(status: Option<i32>, stdout: &[u8], stderr: &[u8]) -> HerdrError {
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = String::from_utf8_lossy(stdout);
    let text = if stderr.is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let text = if text.is_empty() {
        status.map_or_else(|| "killed by a signal".to_owned(), |c| format!("exit {c}"))
    } else {
        text.to_owned()
    };
    if let Ok(Value::Object(mut reply)) = serde_json::from_str::<Value>(&text)
        && let Some(error) = reply.remove("error")
    {
        let err = wire::refusal(error);
        return if err.is_code(SERVER_NOT_RUNNING) {
            HerdrError::NotRunning
        } else {
            err
        };
    }
    HerdrError::Refused {
        code: String::new(),
        message: text,
    }
}

/// stdout as Python's `text=True` reads it (universal newlines).
fn text_mode(stdout: &[u8]) -> String {
    super::calls::universal_newlines(&String::from_utf8_lossy(stdout))
}

/// How often a spawn is retried when the binary is busy being written
/// (`ETXTBSY`: herdr mid-update, or a file another thread's fork briefly
/// holds open). Nothing ran, so a retry is safe.
const SPAWN_TRIES: u32 = 5;
const SPAWN_RETRY: Duration = Duration::from_millis(20);

fn busy(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::ExecutableFileBusy
}

fn spawn_error(bin: &Path, e: &std::io::Error) -> HerdrError {
    HerdrError::Unreachable(format!("{}: {e}", bin.display()))
}

fn timeout_error(timeout: Duration) -> HerdrError {
    HerdrError::NoReply(format!(
        "herdr CLI timed out after {:.1} s",
        timeout.as_secs_f64()
    ))
}

/// Run one invocation (async); its stdout on success.
pub(crate) async fn run(
    bin: &Path,
    inv: &Invocation,
    timeout: Duration,
) -> Result<String, HerdrError> {
    let timeout = inv.timeout.unwrap_or(timeout);
    let mut command = tokio::process::Command::new(bin);
    command
        .args(&inv.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut tries = 0;
    let child = loop {
        match command.spawn() {
            Err(e) if busy(&e) && tries < SPAWN_TRIES => {
                tries += 1;
                tokio::time::sleep(SPAWN_RETRY).await;
            }
            spawned => break spawned.map_err(|e| spawn_error(bin, &e))?,
        }
    };
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Err(_) => Err(timeout_error(timeout)),
        Ok(Err(e)) => Err(HerdrError::NoReply(e.to_string())),
        Ok(Ok(out)) if out.status.success() => Ok(text_mode(&out.stdout)),
        Ok(Ok(out)) => Err(exit_error(out.status.code(), &out.stdout, &out.stderr)),
    }
}

/// Run one invocation (blocking); its stdout on success.
pub(crate) fn run_blocking(
    bin: &Path,
    inv: &Invocation,
    timeout: Duration,
) -> Result<String, HerdrError> {
    let timeout = inv.timeout.unwrap_or(timeout);
    let mut command = Command::new(bin);
    command
        .args(&inv.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tries = 0;
    let mut child = loop {
        match command.spawn() {
            Err(e) if busy(&e) && tries < SPAWN_TRIES => {
                tries += 1;
                std::thread::sleep(SPAWN_RETRY);
            }
            spawned => break spawned.map_err(|e| spawn_error(bin, &e))?,
        }
    };
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(timeout_error(timeout));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                let _ = child.kill();
                return Err(HerdrError::NoReply(e.to_string()));
            }
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if status.success() {
        Ok(text_mode(&stdout))
    } else {
        Err(exit_error(status.code(), &stdout, &stderr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_argument_is_cleaned() {
        let inv = machine_focus_agent("m\n1", &PaneId::new("w1:p1\x1b[2J"));
        assert_eq!(inv.args, ["--machine", "m 1", "agent", "focus", "w1:p1[2J"]);
    }

    #[test]
    fn exit_errors_keep_herdrs_code_or_its_text() {
        let e = exit_error(
            Some(1),
            b"",
            br#"{"error":{"code":"agent_not_found","message":"agent target w1:p3 not found"},"id":"cli:agent:get"}"#,
        );
        assert!(e.is_code("agent_not_found"));
        let e = exit_error(
            Some(2),
            b"",
            b"error: unknown machine 'nosuch'; use `herdr machine list`\n",
        );
        assert_eq!(
            e.to_string(),
            "error: unknown machine 'nosuch'; use `herdr machine list`"
        );
        assert_eq!(exit_error(Some(3), b"", b"").to_string(), "exit 3");
        assert_eq!(exit_error(Some(1), b" out \n", b"").to_string(), "out");
    }

    /// herdr-inbox tests/test_successor.py::test_server_gone_needs_herdrs_own_word
    /// (the stderr shape is that test's fake herdr: no recording has a stopped
    /// server).
    #[test]
    fn only_server_not_running_means_not_running() {
        let gone = br#"{"id": "cli:workspace:list", "error": {"code": "server_not_running", "message": "x"}}"#;
        assert_eq!(exit_error(Some(1), b"", gone), HerdrError::NotRunning);
        let other = br#"{"error": {"code": "other", "message": "x"}}"#;
        assert!(exit_error(Some(1), b"", other).is_code("other"));
        assert!(matches!(
            exit_error(Some(1), b"", b"not json"),
            HerdrError::Refused { .. }
        ));
    }

    #[test]
    fn machine_list_reads_a_list_or_an_envelope() {
        assert_eq!(parse_machines("[]\n").unwrap(), vec![]);
        assert_eq!(parse_machines("").unwrap(), vec![]);
        assert!(parse_machines("nope").is_err());
        let m = parse_machines(
            r#"[{"id": "m1", "label": "box", "target": "u@h", "enabled": true}, 3]"#,
        )
        .unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].target.as_deref(), Some("u@h"));
        let m = parse_machines(r#"{"result": {"machines": [{"id": "m2", "enabled": false}]}}"#)
            .unwrap();
        assert_eq!(m[0].id.as_deref(), Some("m2"));
    }

    #[test]
    fn a_machine_snapshot_needs_a_snapshot() {
        assert!(parse_snapshot(r#"{"result": {"snapshot": {"version": "0.9.3"}}}"#).is_ok());
        assert_eq!(
            parse_snapshot(r#"{"result": {}}"#),
            Err(HerdrError::Garbled("no snapshot".into()))
        );
        assert_eq!(
            parse_snapshot("x"),
            Err(HerdrError::Garbled("unreadable snapshot".into()))
        );
    }

    #[test]
    fn config_dir_is_trimmed() {
        assert_eq!(parse_config_dir("/a/b\n"), Some(PathBuf::from("/a/b")));
        assert_eq!(parse_config_dir("  \n"), None);
    }
}
