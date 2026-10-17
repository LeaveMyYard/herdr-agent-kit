//! [`FakeHerdr`]: the scripted socket server, its clients, its script and
//! what it recorded, and its event subscribers.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::private_dir;
use super::recordings::{exchanges, recording, recordings};
use super::script::{Reply, Request, State};
use super::serve::serve;
use crate::herdr_api::ids::ServerId;
use crate::herdr_api::{Herdr, blocking};

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
