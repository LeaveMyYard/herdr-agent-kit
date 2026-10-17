//! Running an invocation, async (tokio) and blocking (std), each bounded by
//! its timeout.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::Invocation;
use super::output::{exit_error, text_mode};
use crate::herdr_api::error::HerdrError;

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
    // the pipes are read on threads that report back over channels, so the
    // wait for them is bounded too: a grandchild (ssh's ControlPersist master
    // under `--machine`) may hold them open after herdr itself has exited
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            let _ = tx.send(buf);
        });
        rx
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
    let collect = |rx: std::sync::mpsc::Receiver<Vec<u8>>| {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| timeout_error(timeout))
    };
    let stdout = collect(out)?;
    let stderr = collect(err)?;
    if status.success() {
        Ok(text_mode(&stdout))
    } else {
        Err(exit_error(status.code(), &stdout, &stderr))
    }
}
