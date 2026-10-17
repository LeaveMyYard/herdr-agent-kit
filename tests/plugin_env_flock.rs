//! Cross-process exclusion against real `flock(2)` and real Python: a lock
//! held by a Python process (opened and taken exactly as the plugins do)
//! blocks the Rust lock on the same path, and the other way round. The Python
//! side computes each path with the plugins' own formulas, so this also proves
//! that both sides name the same file.
//!
//! The child gets a cleared environment with only the variables set here,
//! pointing into a temporary directory; nothing talks to herdr. Panorama's
//! state directory is given relative to that directory (the child's cwd), so
//! the socket path stays under the 100-character limit however long the
//! temporary directory is, and the real `/tmp/herdr-panorama-<uid>` fallback
//! is never touched.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

use herdr_agent_kit::internal::plugin_paths::{inbox, panorama};
use herdr_agent_kit::plugin_env::{LockFile, PluginEnv};

const SESSION: &str = "1CD70DE7-aaaa-4bbb-8ccc-0123456789AB";

/// The Python side: `python3 -I -c SCRIPT <kind> <action> [session]`.
/// The paths and open calls are copied from herdr-inbox `worker._lock_path` /
/// `worker.hold_lock` / `worker.is_running`, `rewake.run`, and herdr-panorama
/// `protocol.socket_dir` / `protocol.lock_path` / `Core._start`.
const SCRIPT: &str = r#"
import fcntl, hashlib, os, sys
from pathlib import Path

kind, action = sys.argv[1], sys.argv[2]

def key(s):
    return hashlib.sha1(s.encode()).hexdigest()[:12]

if kind == "worker":
    path = Path(os.environ["HERDR_INBOX_DB"]).parent / f"worker-{key(os.environ['HERDR_SOCKET_PATH'])}.lock"
elif kind == "rewake":
    path = Path(os.environ["HERDR_INBOX_DB"]).parent / "rewake" / f"{sys.argv[3].lower()}.lock"
elif kind == "core":
    state = Path(os.environ["HERDR_PLUGIN_STATE_DIR"])
    name = "core-0123456789ab.sock"
    for d in (state / "sockets",
              Path(os.environ.get("XDG_RUNTIME_DIR") or "/tmp") / f"herdr-panorama-{os.getuid()}"):
        if len(str(d / name)) <= 100:
            break
    else:
        d = Path("/tmp") / f"herdr-panorama-{os.getuid()}"
    if d.is_absolute():
        raise SystemExit(f"refusing a socket dir outside the test's directory: {d}")
    path = d / f"core-{key(os.environ.get('HERDR_SOCKET_PATH') or 'default')}.lock"
else:
    raise SystemExit(f"unknown kind {kind}")
print(path, flush=True)

def open_lock():
    if kind == "core":
        try:
            os.makedirs(path.parent.parent, exist_ok=True)
            os.mkdir(path.parent, 0o700)
        except FileExistsError:
            pass
        return os.open(path, os.O_RDWR | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0), 0o600)
    path.parent.mkdir(parents=True, exist_ok=True)
    return open(path, "a")

fh = open_lock()
if action == "hold":
    fcntl.flock(fh, fcntl.LOCK_EX | fcntl.LOCK_NB)
    print("held", flush=True)
    sys.stdin.read()
elif action == "try":
    try:
        fcntl.flock(fh, fcntl.LOCK_EX | fcntl.LOCK_NB)
        print("got", flush=True)
    except OSError:
        print("busy", flush=True)
elif action == "probe":
    try:
        fcntl.flock(fh, fcntl.LOCK_SH | fcntl.LOCK_NB)
        print("free", flush=True)
    except OSError:
        print("busy", flush=True)
"#;

struct Fixture {
    dir: tempfile::TempDir,
    db: PathBuf,
    state: PathBuf,
    socket: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("inbox/inbox.db");
        let state = PathBuf::from("pano"); // relative to `dir`: see the module docs
        let socket = dir.path().join("herdr.sock");
        Fixture {
            dir,
            db,
            state,
            socket,
        }
    }

    fn vars(&self) -> Vec<(&'static str, PathBuf)> {
        vec![
            ("HERDR_INBOX_DB", self.db.clone()),
            ("HERDR_PLUGIN_STATE_DIR", self.state.clone()),
            ("HERDR_SOCKET_PATH", self.socket.clone()),
            ("XDG_RUNTIME_DIR", self.dir.path().to_path_buf()),
        ]
    }

    fn env(&self) -> PluginEnv {
        PluginEnv::from_vars(self.vars(), None, uid())
    }

    /// A path both sides computed, as this process opens it.
    fn abs(&self, path: &Path) -> PathBuf {
        self.dir.path().join(path)
    }

    fn python(&self, kind: &str, action: &str) -> (Child, BufReader<ChildStdout>) {
        let mut cmd = Command::new("python3");
        cmd.env_clear()
            .args(["-I", "-c", SCRIPT, kind, action, SESSION])
            .current_dir(self.dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if let Some(path) = std::env::var_os("PATH") {
            cmd.env("PATH", path);
        }
        for (k, v) in self.vars() {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("python3 is needed for this test");
        let out = BufReader::new(child.stdout.take().unwrap());
        (child, out)
    }

    /// Run a one-shot Python action: (the path it computed, its answer).
    fn ask(&self, kind: &str, action: &str) -> (PathBuf, String) {
        let (mut child, mut out) = self.python(kind, action);
        let path = line(&mut out);
        let answer = line(&mut out);
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
        (PathBuf::from(path), answer)
    }
}

fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn line(out: &mut BufReader<ChildStdout>) -> String {
    let mut s = String::new();
    out.read_line(&mut s).unwrap();
    s.trim_end().to_owned()
}

fn rust_path(kind: &str, env: &PluginEnv) -> PathBuf {
    match kind {
        "worker" => inbox::worker_lock_path(env, None),
        "rewake" => inbox::rewake_lock_path(env, SESSION).unwrap(),
        "core" => panorama::core_lock_path(env),
        _ => unreachable!(),
    }
}

fn rust_lock(kind: &str, path: &Path) -> LockFile {
    match kind {
        "core" => LockFile::private(path),
        _ => LockFile::new(path),
    }
}

fn python_holds_rust_waits(kind: &str) {
    let fx = Fixture::new();
    let env = fx.env();
    let (mut child, mut out) = fx.python(kind, "hold");
    let py_path = PathBuf::from(line(&mut out));
    assert_eq!(line(&mut out), "held", "{kind}");
    let path = rust_path(kind, &env);
    assert_eq!(path, py_path, "{kind}: both sides name the same lock file");

    let lock = rust_lock(kind, &fx.abs(&path));
    assert!(lock.try_exclusive().unwrap().is_none(), "{kind}");
    assert!(lock.held_elsewhere(), "{kind}");
    assert!(lock.held_elsewhere_if_exists(), "{kind}");

    drop(child.stdin.take()); // the holder reads stdin to EOF, then exits
    assert!(child.wait().unwrap().success());
    assert!(!lock.held_elsewhere(), "{kind}");
    assert!(lock.try_exclusive().unwrap().is_some(), "{kind}");
}

fn rust_holds_python_waits(kind: &str) {
    let fx = Fixture::new();
    let env = fx.env();
    let path = rust_path(kind, &env);
    let abs = fx.abs(&path);
    if kind == "core" {
        herdr_agent_kit::plugin_env::private_dir::make_private_dir(abs.parent().unwrap(), uid())
            .unwrap();
    }
    let held = rust_lock(kind, &abs).try_exclusive().unwrap().unwrap();

    let (py_path, answer) = fx.ask(kind, "try");
    assert_eq!(py_path, path, "{kind}");
    assert_eq!(
        answer, "busy",
        "{kind}: Python cannot take a lock Rust holds"
    );
    assert_eq!(
        fx.ask(kind, "probe").1,
        "busy",
        "{kind}: is_running sees it"
    );

    drop(held);
    assert_eq!(fx.ask(kind, "probe").1, "free", "{kind}");
    assert_eq!(fx.ask(kind, "try").1, "got", "{kind}");
}

#[test]
fn worker_lock_excludes_python() {
    python_holds_rust_waits("worker");
    rust_holds_python_waits("worker");
}

#[test]
fn rewake_lock_excludes_python() {
    python_holds_rust_waits("rewake");
    rust_holds_python_waits("rewake");
}

#[test]
fn core_lock_excludes_python() {
    python_holds_rust_waits("core");
    rust_holds_python_waits("core");
}

#[test]
fn a_python_probe_does_not_break_a_rust_retry() {
    // The worker handover contract: Python probes with LOCK_SH for an instant;
    // the Rust worker retries LOCK_EX|LOCK_NB for WORKER_LOCK_RETRY.
    let fx = Fixture::new();
    let env = fx.env();
    let lock = LockFile::new(inbox::worker_lock_path(&env, None));
    let (mut child, mut out) = fx.python("worker", "hold");
    line(&mut out);
    assert_eq!(line(&mut out), "held");
    let got = lock
        .exclusive_within(
            inbox::WORKER_LOCK_RETRY,
            std::time::Duration::from_millis(20),
            || true,
        )
        .unwrap();
    assert!(
        got.is_none(),
        "another worker holds it: give up after the retry window"
    );
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
    let got = lock
        .exclusive_within(
            inbox::WORKER_LOCK_RETRY,
            std::time::Duration::from_millis(20),
            || true,
        )
        .unwrap();
    assert!(got.is_some());
}
