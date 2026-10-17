//! [`Tailer`]: incremental reading of an append-only file.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

/// A partial line longer than this is dropped, and reading resyncs at the
/// next newline.
pub const MAX_PARTIAL: usize = 8 * 1024 * 1024;

/// What one [`Tailer::read`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Tail {
    /// The file does not exist or cannot be read.
    Missing,
    /// Nothing was appended.
    NoChange,
    /// The file shrank or was replaced: forget everything derived from it,
    /// then read again (from the start).
    Reset,
    /// The complete lines appended since the last read (possibly none), CRLF
    /// and blank lines dropped.
    Lines(Vec<Vec<u8>>),
}

/// Incremental reading of an append-only file: panorama's `scan/tail.py`
/// (after zoetrope's `bytes.rs`, MIT). Stat, read the bytes appended past the
/// last offset, split on `\n`, return complete lines and buffer the trailing
/// partial line. A file that shrank or was replaced (its `(dev, inode)`
/// changed) resets to offset 0. Lines are bytes: callers decode only the
/// lines they parse.
#[derive(Clone, Debug, Default)]
pub struct Tailer {
    offset: u64,
    partial: Vec<u8>,
    overflowed: bool,
    identity: Option<(u64, u64)>,
    mtime: Option<SystemTime>,
    size: u64,
}

impl Tailer {
    /// A tailer at the start of a file it has never seen.
    pub fn new() -> Tailer {
        Tailer::default()
    }

    /// The bytes read so far.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The file's size at the last read that found it unchanged or grown.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Its modification time then.
    pub fn mtime(&self) -> Option<SystemTime> {
        self.mtime
    }

    /// The `(dev, inode)` of the file read, once read (always `None` off Unix).
    pub fn identity(&self) -> Option<(u64, u64)> {
        self.identity
    }

    /// Reads what was appended to `path` since the last read.
    pub fn read(&mut self, path: &Path) -> Tail {
        self.read_at_most(path, u64::MAX)
    }

    /// [`Tailer::read`], reading at most `limit` bytes (at least one): a
    /// large file is read in bounded steps, each returning its complete
    /// lines; the next call continues where this one stopped.
    pub fn read_at_most(&mut self, path: &Path, limit: u64) -> Tail {
        let Ok(meta) = fs::metadata(path) else {
            return Tail::Missing;
        };
        let identity = file_identity(&meta);
        let replaced = self.identity.is_some() && identity != self.identity;
        if meta.len() < self.offset || replaced {
            self.offset = 0;
            self.overflowed = false;
            self.identity = identity;
            self.partial.clear();
            return Tail::Reset;
        }
        self.mtime = meta.modified().ok();
        self.size = meta.len();
        self.identity = identity;
        if meta.len() == self.offset {
            return Tail::NoChange;
        }
        let mut buf = Vec::new();
        let read = File::open(path).and_then(|mut f| {
            f.seek(SeekFrom::Start(self.offset))?;
            f.take((meta.len() - self.offset).min(limit.max(1)))
                .read_to_end(&mut buf)
        });
        if read.is_err() {
            return Tail::Missing;
        }
        self.offset += buf.len() as u64;
        Tail::Lines(self.consume(&buf))
    }

    /// Splits appended bytes into complete lines and buffers the trailing
    /// partial line (pure over the byte stream).
    pub fn consume(&mut self, appended: &[u8]) -> Vec<Vec<u8>> {
        let mut lines = Vec::new();
        let mut start = 0;
        while let Some(rel) = memchr::memchr(b'\n', &appended[start..]) {
            let i = start + rel;
            if self.overflowed {
                self.overflowed = false;
                self.partial.clear();
            } else if !self.partial.is_empty() {
                self.partial.extend_from_slice(&appended[start..i]);
                push_line(&mut lines, &self.partial);
                self.partial.clear();
            } else {
                push_line(&mut lines, &appended[start..i]);
            }
            start = i + 1;
        }
        if !self.overflowed && start < appended.len() {
            self.partial.extend_from_slice(&appended[start..]);
            if self.partial.len() > MAX_PARTIAL {
                self.partial = Vec::new();
                self.overflowed = true;
            }
        }
        lines
    }
}

fn push_line(lines: &mut Vec<Vec<u8>>, raw: &[u8]) {
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    if !crate::agent_session::claude::wire::blank(raw) {
        lines.push(raw.to_vec());
    }
}

#[cfg(unix)]
fn file_identity(meta: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_identity(_meta: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests;
