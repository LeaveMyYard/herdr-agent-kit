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

mod cli_stub;
mod recordings;
mod script;
mod serve;
mod server;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub use cli_stub::{CliReply, CliStub};
pub use recordings::{exchanges, recording, recordings};
pub use script::{Reply, Request};
pub use server::FakeHerdr;

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
