//! Liveness against this machine's real process table.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use herdr_agent_kit::agent_procs::{Liveness, SystemLiveness, descendants, live_tasks};

/// A private directory under the build's tmp dir (never the system /tmp).
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("agent-procs-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A background shell below this process holding `…/tasks/<id>.output` open,
/// as Claude Code's background Bash does, is a live task; once it exits, it
/// is not. Without procfs (macOS) tasks are unknown, never dead.
#[test]
fn a_background_shell_holding_its_output_file_is_a_live_task() {
    let me = std::process::id();
    if fs::metadata("/proc/self/stat").is_err() {
        assert_eq!(live_tasks(me), None);
        assert_eq!(SystemLiveness::new(Some(me)).task_alive("b7k2"), None);
        return;
    }
    let dir = scratch("tasks");
    let tasks = dir.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    let out = tasks.join("b7k2.output");
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("exec 3>\"$1\"; echo ready; read x")
        .arg("sh")
        .arg(&out)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = [0u8; 6];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut ready)
        .unwrap();

    assert!(descendants(me).unwrap().contains(&child.id()));
    assert!(live_tasks(me).unwrap().contains("b7k2"));
    let live = SystemLiveness::new(Some(me));
    assert_eq!(live.task_alive("b7k2"), Some(true));
    assert_eq!(live.task_alive("other"), Some(false));

    drop(child.stdin.take()); // `read x` ends, the shell exits
    child.wait().unwrap();
    assert!(!live_tasks(me).unwrap().contains("b7k2"));
    // the earlier value keeps its pass's answer; a new pass sees the exit
    assert_eq!(live.task_alive("b7k2"), Some(true));
    assert_eq!(
        SystemLiveness::new(Some(me)).task_alive("b7k2"),
        Some(false)
    );
    // an unreadable process: unknown
    assert_eq!(live_tasks(u32::MAX), None);
    assert_eq!(descendants(u32::MAX), None);
    let _ = fs::remove_dir_all(&dir);
}
