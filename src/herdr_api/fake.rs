//! `FakeHerdr`, behind the `test-support` feature. **Test IO.**
//!
//! [`FakeHerdr`] is a Unix socket server that answers like herdr and records
//! every request as `(method, params)`. Integration tests in both plugins use
//! it in place of an argv-recording CLI stub. [`FakeHerdr::recorded`] seeds it
//! with every request and reply in the herdr socket recordings of
//! `fixtures/herdr` (captured on an isolated herdr 0.9.3), embedded in the
//! crate so a plugin's tests need no path to them.
//!
//! [`CliStub`] is the CLI path's counterpart: a `herdr` shell script that
//! answers scripted argv and logs every call.
//!
//! Both run on plain threads, so blocking and async tests use them alike.
//! Neither ever talks to a real herdr.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::ids::ServerId;
use super::{Herdr, blocking};

/// The herdr recordings, embedded: `(file name, contents)`.
const RECORDINGS: &[(&str, &str)] = &[
    (
        "agent.jsonl",
        include_str!("../../fixtures/herdr/agent.jsonl"),
    ),
    ("api.jsonl", include_str!("../../fixtures/herdr/api.jsonl")),
    (
        "errors.jsonl",
        include_str!("../../fixtures/herdr/errors.jsonl"),
    ),
    (
        "events.jsonl",
        include_str!("../../fixtures/herdr/events.jsonl"),
    ),
    (
        "machine.jsonl",
        include_str!("../../fixtures/herdr/machine.jsonl"),
    ),
    (
        "meta.jsonl",
        include_str!("../../fixtures/herdr/meta.jsonl"),
    ),
    (
        "notification.jsonl",
        include_str!("../../fixtures/herdr/notification.jsonl"),
    ),
    (
        "pane.jsonl",
        include_str!("../../fixtures/herdr/pane.jsonl"),
    ),
    (
        "plugin.jsonl",
        include_str!("../../fixtures/herdr/plugin.jsonl"),
    ),
    (
        "server.jsonl",
        include_str!("../../fixtures/herdr/server.jsonl"),
    ),
    (
        "socket.jsonl",
        include_str!("../../fixtures/herdr/socket.jsonl"),
    ),
    ("tab.jsonl", include_str!("../../fixtures/herdr/tab.jsonl")),
    (
        "workspace.jsonl",
        include_str!("../../fixtures/herdr/workspace.jsonl"),
    ),
];

/// Every record of the embedded recordings, parsed: `(file name, record)`.
pub fn recordings() -> Vec<(&'static str, Value)> {
    RECORDINGS
        .iter()
        .flat_map(|(file, text)| {
            text.lines().filter(|l| !l.trim().is_empty()).map(|l| {
                (
                    *file,
                    serde_json::from_str(l).expect("recordings are JSON lines"),
                )
            })
        })
        .collect()
}

/// One record: `file` is `agent.jsonl`, `seq` its `seq`.
pub fn recording(file: &str, seq: u64) -> Option<Value> {
    recordings()
        .into_iter()
        .find(|(f, r)| *f == file && r["seq"].as_u64() == Some(seq))
        .map(|(_, r)| r)
}

/// The socket `(request frame, reply frame)` pairs of one record, in order
/// (the CLI's `ping` included).
pub fn exchanges(record: &Value) -> Vec<(Value, Value)> {
    let frames = record["socket"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for (i, f) in frames.iter().enumerate() {
        if f["dir"] != "request" || f.get("frame").is_none() {
            continue;
        }
        let reply = frames[i + 1..]
            .iter()
            .find(|r| r["conn"] == f["conn"] && r["dir"] == "reply" && r.get("frame").is_some());
        if let Some(reply) = reply {
            out.push((f["frame"].clone(), reply["frame"].clone()));
        }
    }
    out
}

/// How the fake answers one request.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// `{"id", "result": value}`.
    Result(Value),
    /// `{"id", "error": {"code", "message"}}`.
    Error {
        /// herdr's error code.
        code: String,
        /// herdr's message.
        message: String,
    },
    /// Close the connection without a reply.
    Close,
    /// Send these bytes as they are (a garbled reply), then close.
    Raw(Vec<u8>),
    /// Never answer: hold the connection until the client gives up.
    Hang,
}

impl Reply {
    /// A refusal with herdr's error code.
    pub fn error(code: &str, message: &str) -> Self {
        Self::Error {
            code: code.to_owned(),
            message: message.to_owned(),
        }
    }

    /// A recorded reply frame as a reply.
    fn from_frame(frame: &Value) -> Self {
        match frame.get("error") {
            Some(e) => Self::Error {
                code: e["code"].as_str().unwrap_or_default().to_owned(),
                message: e["message"].as_str().unwrap_or_default().to_owned(),
            },
            None => Self::Result(frame.get("result").cloned().unwrap_or(Value::Null)),
        }
    }
}

/// One request as the fake received it.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// The request's `id` (`Null` when missing).
    pub id: Value,
    /// The method (empty when the line was not a JSON request).
    pub method: String,
    /// The params (`Null` when missing).
    pub params: Value,
    /// The line as received, without its newline.
    pub line: String,
}

#[derive(Default)]
struct State {
    /// `(method, params, reply)` from the recordings; the last one wins.
    recorded: Vec<(String, Value, Reply)>,
    sticky: HashMap<String, Reply>,
    once: HashMap<String, VecDeque<Reply>>,
    requests: Vec<Request>,
    subscribers: Vec<UnixStream>,
}

impl State {
    fn reply_for(&mut self, method: &str, params: &Value) -> Reply {
        if let Some(r) = self.once.get_mut(method).and_then(VecDeque::pop_front) {
            return r;
        }
        if let Some(r) = self.sticky.get(method) {
            return r.clone();
        }
        if method == "events.subscribe" {
            return Reply::Result(json!({"type": "subscription_started"}));
        }
        let exact = self
            .recorded
            .iter()
            .rev()
            .find(|(m, p, _)| m == method && p == params);
        let found = exact.or_else(|| self.recorded.iter().rev().find(|(m, _, _)| m == method));
        if let Some((_, _, r)) = found {
            return r.clone();
        }
        Reply::error(
            "invalid_request",
            &format!("FakeHerdr: no reply scripted or recorded for `{method}`"),
        )
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A fresh, short, private directory for a socket or a stub (macOS caps
/// socket paths at 104 bytes).
fn private_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let dir = std::env::temp_dir().join(format!(
        "{tag}{}-{}-{nanos:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("create the fake's directory");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .expect("chmod the fake's directory");
    dir
}

/// A scripted herdr socket server. Stops, and removes its directory, when
/// dropped.
///
/// A request is answered by, in order: a reply queued with
/// [`FakeHerdr::once`]; one set with [`FakeHerdr::on`]; for
/// `events.subscribe`, herdr's acknowledgement (the connection then stays
/// open as a subscriber); the recorded reply to the same method and params;
/// the last recorded reply to the same method; else an `invalid_request`
/// refusal. The recordings are one session in order, so "the recorded reply"
/// is the latest one; [`FakeHerdr::replay`] picks an earlier exchange.
pub struct FakeHerdr {
    dir: PathBuf,
    socket: PathBuf,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl FakeHerdr {
    /// A fake with nothing scripted or recorded.
    pub fn start() -> Self {
        Self::with_state(State::default())
    }

    /// A fake seeded with every socket exchange in the recordings.
    pub fn recorded() -> Self {
        let mut state = State::default();
        for (_, record) in recordings() {
            for (request, reply) in exchanges(&record) {
                let method = request["method"].as_str().unwrap_or_default();
                if method.is_empty() || method == "ping" || method == "events.subscribe" {
                    continue;
                }
                state.recorded.push((
                    method.to_owned(),
                    request.get("params").cloned().unwrap_or(Value::Null),
                    Reply::from_frame(&reply),
                ));
            }
        }
        Self::with_state(state)
    }

    fn with_state(state: State) -> Self {
        let dir = private_dir("hf");
        let socket = dir.join("s");
        let listener = UnixListener::bind(&socket).expect("bind the fake's socket");
        let state = Arc::new(Mutex::new(state));
        let stop = Arc::new(AtomicBool::new(false));
        let accept = {
            let (state, stop) = (state.clone(), stop.clone());
            std::thread::spawn(move || {
                for conn in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(conn) = conn {
                        let (state, stop) = (state.clone(), stop.clone());
                        std::thread::spawn(move || serve(conn, &state, &stop));
                    }
                }
            })
        };
        Self {
            dir,
            socket,
            state,
            stop,
            accept: Some(accept),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The socket path.
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// This fake as a server id.
    pub fn server(&self) -> ServerId {
        ServerId::new(&self.socket)
    }

    /// An async client of this fake.
    pub fn client(&self) -> Herdr {
        Herdr::new(self.server())
    }

    /// A blocking client of this fake.
    pub fn blocking_client(&self) -> blocking::Herdr {
        blocking::Herdr::new(self.server())
    }

    /// Answer every later `method` request with `reply`.
    pub fn on(&self, method: &str, reply: Reply) -> &Self {
        self.lock().sticky.insert(method.to_owned(), reply);
        self
    }

    /// Answer the next `method` request with `reply`, once.
    pub fn once(&self, method: &str, reply: Reply) -> &Self {
        self.lock()
            .once
            .entry(method.to_owned())
            .or_default()
            .push_back(reply);
        self
    }

    /// Queue the replies of one recorded exchange (`file` `agent.jsonl`,
    /// `seq` its `seq`) as [`FakeHerdr::once`] replies, so its requests get
    /// exactly the replies herdr gave then (the recordings hold one session,
    /// so the same request has different answers at different times).
    /// Panics if there is no such record.
    pub fn replay(&self, file: &str, seq: u64) -> &Self {
        let record = recording(file, seq).unwrap_or_else(|| panic!("no recording {file}:{seq}"));
        for (request, reply) in exchanges(&record) {
            let method = request["method"].as_str().unwrap_or_default();
            if !method.is_empty() && method != "ping" {
                self.once(method, Reply::from_frame(&reply));
            }
        }
        self
    }

    /// Every request so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.lock().requests.clone()
    }

    /// Every request so far as `(method, params)`.
    pub fn calls(&self) -> Vec<(String, Value)> {
        self.lock()
            .requests
            .iter()
            .map(|r| (r.method.clone(), r.params.clone()))
            .collect()
    }

    /// The params of every `method` request so far.
    pub fn params_of(&self, method: &str) -> Vec<Value> {
        self.lock()
            .requests
            .iter()
            .filter(|r| r.method == method)
            .map(|r| r.params.clone())
            .collect()
    }

    /// Live `events.subscribe` connections.
    pub fn subscribers(&self) -> usize {
        self.lock().subscribers.len()
    }

    /// Wait until at least `n` subscribers are connected; false on timeout.
    pub fn wait_for_subscribers(&self, n: usize, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.subscribers() >= n {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.subscribers() >= n
    }

    /// Send an event to every subscriber, framed as herdr frames it:
    /// `{"data": data, "event": data.type}`. herdr filters events by
    /// subscription; the fake sends every event to every subscriber.
    pub fn emit(&self, data: Value) {
        let event = data.get("type").cloned().unwrap_or(Value::Null);
        self.send_line(&json!({"data": data, "event": event}).to_string());
    }

    /// Send one raw line (a newline is added) to every subscriber.
    pub fn send_line(&self, line: &str) {
        let bytes = format!("{line}\n");
        // written outside the lock: a subscriber that stops reading must not
        // wedge the whole fake
        let mut subs = std::mem::take(&mut self.lock().subscribers);
        subs.retain_mut(|s| s.write_all(bytes.as_bytes()).is_ok());
        self.lock().subscribers.extend(subs);
    }

    /// Close every subscriber's connection (herdr going away).
    pub fn drop_subscribers(&self) {
        for s in self.lock().subscribers.drain(..) {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl Drop for FakeHerdr {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.drop_subscribers();
        // wake the accept loop (a test may have made the socket unusable;
        // if it stays so, the loop is left behind rather than waited for)
        let _ = std::fs::set_permissions(&self.socket, std::fs::Permissions::from_mode(0o600));
        if UnixStream::connect(&self.socket).is_ok()
            && let Some(t) = self.accept.take()
        {
            let _ = t.join();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn frame(id: &Value, reply: &Reply) -> Option<String> {
    let frame = match reply {
        Reply::Result(v) => json!({"id": id, "result": v}),
        Reply::Error { code, message } => {
            json!({"id": id, "error": {"code": code, "message": message}})
        }
        Reply::Close | Reply::Raw(_) | Reply::Hang => return None,
    };
    Some(format!("{frame}\n"))
}

/// One connection: requests line by line until it closes.
fn serve(conn: UnixStream, state: &Mutex<State>, stop: &AtomicBool) {
    let lock = || state.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(mut out) = conn.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(conn);
    let mut line = String::new();
    loop {
        line.clear();
        if matches!(reader.read_line(&mut line), Ok(0) | Err(_)) || stop.load(Ordering::SeqCst) {
            return;
        }
        let text = line.trim_end_matches('\n').to_owned();
        let Some(req) = serde_json::from_str::<Value>(&text)
            .ok()
            .filter(Value::is_object)
        else {
            lock().requests.push(Request {
                id: Value::Null,
                method: String::new(),
                params: Value::Null,
                line: text,
            });
            // herdr answers a line it cannot parse with an empty id
            let reply = Reply::error("invalid_request", "invalid request: not a JSON request");
            match frame(&json!(""), &reply) {
                Some(f) if out.write_all(f.as_bytes()).is_ok() => continue,
                _ => return,
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req["method"].as_str().unwrap_or_default().to_owned();
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let reply = {
            let mut st = lock();
            st.requests.push(Request {
                id: id.clone(),
                method: method.clone(),
                params: params.clone(),
                line: text,
            });
            st.reply_for(&method, &params)
        };
        let reply_id = if id.is_null() { json!("") } else { id };
        match &reply {
            Reply::Close => return,
            Reply::Raw(bytes) => {
                let _ = out.write_all(bytes);
                return;
            }
            Reply::Hang => {
                // until the client closes its end
                let mut rest = Vec::new();
                let _ = std::io::Read::read_to_end(&mut reader, &mut rest);
                return;
            }
            Reply::Result(_) | Reply::Error { .. } => {
                let Some(f) = frame(&reply_id, &reply) else {
                    return;
                };
                // the subscriber joins before its ack goes out, so an event
                // emitted right after the client saw the ack reaches it
                if method == "events.subscribe" && matches!(reply, Reply::Result(_)) {
                    let mut st = lock();
                    if out.write_all(f.as_bytes()).is_err() {
                        return;
                    }
                    if let Ok(sub) = out.try_clone() {
                        st.subscribers.push(sub);
                    }
                } else if out.write_all(f.as_bytes()).is_err() {
                    return;
                }
            }
        }
    }
}

// -- the CLI path ----------------------------------------------------------------

/// What [`CliStub`] does for one argv.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CliReply {
    /// Printed on stdout.
    pub stdout: String,
    /// Printed on stderr.
    pub stderr: String,
    /// The exit status.
    pub exit: u8,
    /// Slept first (for timeouts).
    pub delay: Option<Duration>,
}

impl CliReply {
    /// Exit 0 with this stdout.
    pub fn ok(stdout: &str) -> Self {
        Self {
            stdout: stdout.to_owned(),
            ..Self::default()
        }
    }
}

/// A fake `herdr` binary: a shell script that answers scripted argv and logs
/// every call. Unscripted argv exits 2 with a message on stderr. Removes its
/// directory when dropped.
pub struct CliStub {
    dir: PathBuf,
    replies: Vec<(Vec<String>, CliReply)>,
}

/// The separator of logged arguments: a control character, which
/// [`clean`](super::clean)ed arguments never contain.
const SEP: char = '\u{1f}';

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

impl CliStub {
    /// A stub with nothing scripted.
    pub fn new() -> Self {
        let stub = Self {
            dir: private_dir("hc"),
            replies: Vec::new(),
        };
        Self::install(&stub.dir);
        stub.write();
        stub
    }

    /// A stub that answers every CLI record of the recordings (its argv
    /// after `herdr`) with the recorded stdout, stderr and exit status.
    pub fn recorded() -> Self {
        let mut stub = Self::new();
        for (_, r) in recordings() {
            if r["kind"] != "cli" {
                continue;
            }
            let argv: Vec<String> = r["argv"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .skip(1)
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let reply = CliReply {
                stdout: r["stdout"].as_str().unwrap_or_default().to_owned(),
                stderr: r["stderr"].as_str().unwrap_or_default().to_owned(),
                exit: r["exit_code"]
                    .as_u64()
                    .and_then(|c| u8::try_from(c).ok())
                    .unwrap_or(1),
                delay: None,
            };
            stub.replies.retain(|(a, _)| *a != argv);
            stub.replies.push((argv, reply));
        }
        stub.write();
        stub
    }

    /// Answer `args` (after `herdr`) with `reply` from now on.
    pub fn on(&mut self, args: &[&str], reply: CliReply) -> &mut Self {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        self.replies.retain(|(a, _)| *a != args);
        self.replies.push((args, reply));
        self.write();
        self
    }

    /// The stub's path, for `with_bin`.
    pub fn bin(&self) -> PathBuf {
        self.dir.join("herdr")
    }

    /// Every call so far: its arguments after `herdr`.
    pub fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(|l| {
                l.strip_suffix(SEP)
                    .map(|l| l.split(SEP).map(str::to_owned).collect())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The executable, written once: it sources `rules.sh`, which
    /// [`CliStub::on`] rewrites. Rewriting a file that is exec'd races with
    /// other threads' forks (`ETXTBSY`); a sourced file is only read.
    fn install(dir: &Path) {
        let rules = sh_quote(&dir.join("rules.sh").to_string_lossy());
        let path = dir.join("herdr");
        std::fs::write(&path, format!("#!/bin/sh\n. {rules}\n")).expect("write the stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod the stub");
    }

    fn write(&self) {
        let log = sh_quote(&self.dir.join("calls.log").to_string_lossy());
        let mut script = format!(
            "key=$(for a in \"$@\"; do printf '%s\\037' \"$a\"; done)\n\
             printf '%s\\n' \"$key\" >> {log}\n\
             case \"$key\" in\n"
        );
        for (args, reply) in &self.replies {
            let key: String = args.iter().map(|a| format!("{a}{SEP}")).collect();
            script.push_str(&format!("  {})\n", sh_quote(&key)));
            if let Some(d) = reply.delay {
                script.push_str(&format!("    sleep {:.3}\n", d.as_secs_f64()));
            }
            script.push_str(&format!(
                "    printf '%s' {}\n    printf '%s' {} >&2\n    exit {}\n    ;;\n",
                sh_quote(&reply.stdout),
                sh_quote(&reply.stderr),
                reply.exit
            ));
        }
        script.push_str("esac\necho \"fake herdr: nothing scripted for: $*\" >&2\nexit 2\n");
        let tmp = self.dir.join("rules.tmp");
        std::fs::write(&tmp, script).expect("write the stub's rules");
        std::fs::rename(&tmp, self.dir.join("rules.sh")).expect("install the stub's rules");
    }
}

impl Default for CliStub {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CliStub {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
