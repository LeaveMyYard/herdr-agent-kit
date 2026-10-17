//! Read-only reader of the inbox database, behind the `sqlite` feature. **IO.**
//!
//! Uses only the original 0.1 `questions` columns, opens read-only, and treats
//! a missing file, a lock or a missing table as "no inbox", never as an error.
//!
//! This is herdr-panorama's `inbox.py`: the `?N` question counts on its cards. It never
//! writes, so it never touches the inbox's schema stamp. Goldens:
//! `fixtures/inbox/questions-db/` (databases made by the inbox's own code, as SQL dumps,
//! and what panorama's reader returned for each).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};

/// How long a read waits for a writer's lock before it reads as "no inbox" (Python's
/// `sqlite3.connect(timeout=1.0)`).
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(1);

/// An open question, from the 0.1 columns of `questions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenQuestion {
    /// The question's id (`Q{id}`).
    pub id: i64,
    /// The asking Claude Code session, if known.
    pub session_id: Option<String>,
    /// The asking herdr pane, if known.
    pub pane_id: Option<String>,
    /// Who the agent said asked (`asked_by`), if given.
    pub asked_by: Option<String>,
    /// The question text (empty if the column is empty).
    pub question: String,
    /// When it was asked, as the inbox stored it (ISO 8601).
    pub created_at: Option<String>,
}

/// Where the inbox database is: `$HERDR_INBOX_DB`, else
/// `$XDG_STATE_HOME/herdr-inbox/inbox.db`, else `~/.local/state/herdr-inbox/inbox.db`.
/// Empty variables count as unset. Reads the environment.
pub fn db_path() -> PathBuf {
    db_path_from(
        std::env::var_os("HERDR_INBOX_DB"),
        std::env::var_os("XDG_STATE_HOME"),
        std::env::home_dir(),
    )
}

/// [`db_path`] from given values instead of the environment.
pub fn db_path_from(
    inbox_db: Option<OsString>,
    xdg_state_home: Option<OsString>,
    home: Option<PathBuf>,
) -> PathBuf {
    if let Some(db) = inbox_db.filter(|v| !v.is_empty()) {
        return PathBuf::from(db);
    }
    let state = match xdg_state_home.filter(|v| !v.is_empty()) {
        Some(state) => PathBuf::from(state),
        None => home
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".local")
            .join("state"),
    };
    state.join("herdr-inbox").join("inbox.db")
}

/// The open questions, oldest first, or `None` when there is no usable inbox: no regular
/// file at `path`, one that cannot be opened read-only, a lock held past
/// [`BUSY_TIMEOUT`], no `questions` table, or any other SQLite error.
pub fn open_questions(path: &Path) -> Option<Vec<OpenQuestion>> {
    if !path.is_file() {
        return None;
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let con = Connection::open_with_flags(path, flags).ok()?;
    con.busy_timeout(BUSY_TIMEOUT).ok()?;
    let mut stmt = con
        .prepare(
            "SELECT id, session_id, pane_id, asked_by, question, created_at \
             FROM questions WHERE status = 'open' ORDER BY id",
        )
        .ok()?;
    let rows = stmt
        .query_map([], |r| {
            Ok(OpenQuestion {
                id: r.get(0)?,
                session_id: text(r.get_ref(1)?),
                pane_id: text(r.get_ref(2)?),
                asked_by: text(r.get_ref(3)?),
                question: text(r.get_ref(4)?).unwrap_or_default(),
                created_at: text(r.get_ref(5)?),
            })
        })
        .ok()?;
    rows.collect::<Result<Vec<_>, _>>().ok()
}

/// A column as text. The 0.1 columns have TEXT affinity, so SQLite stores numbers given
/// to them as text; a blob (never written by the inbox) is read as lossy UTF-8.
fn text(v: ValueRef<'_>) -> Option<String> {
    match v {
        ValueRef::Null => None,
        ValueRef::Integer(n) => Some(n.to_string()),
        ValueRef::Real(f) => Some(f.to_string()),
        ValueRef::Text(t) | ValueRef::Blob(t) => Some(String::from_utf8_lossy(t).into_owned()),
    }
}

#[cfg(test)]
mod tests;
