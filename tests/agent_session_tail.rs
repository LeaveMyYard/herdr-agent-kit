//! `agent_session::tail::Tailer` against real files (herdr-panorama
//! `tests/test_scan_tail.py`): appends, a shrunk or replaced file, a missing
//! one.

use std::fs::{self, OpenOptions};
use std::io::Write;

use herdr_agent_kit::agent_session::tail::{Tail, Tailer};

fn lines(v: &[&[u8]]) -> Tail {
    Tail::Lines(v.iter().map(|l| l.to_vec()).collect())
}

#[test]
fn read_appended_offsets_shrink_and_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("t.jsonl");
    fs::write(&p, b"one\ntw").unwrap();
    let mut t = Tailer::new();
    assert_eq!(t.read(&p), lines(&[b"one"]));
    assert_eq!(t.read(&p), Tail::NoChange);
    OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(b"o\nthree\n")
        .unwrap();
    assert_eq!(t.read(&p), lines(&[b"two", b"three"]));
    fs::write(&p, b"new\n").unwrap(); // shrank: rewritten
    assert_eq!(t.read(&p), Tail::Reset);
    assert_eq!(t.read(&p), lines(&[b"new"]));
    let other = dir.path().join("o.jsonl");
    fs::write(&other, b"new\nlonger file\n").unwrap();
    fs::rename(&other, &p).unwrap(); // replaced by a different inode, not shorter
    assert_eq!(t.read(&p), Tail::Reset);
    assert_eq!(t.read(&p), lines(&[b"new", b"longer file"]));
}

#[test]
fn missing_file() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Tailer::new().read(&dir.path().join("nope")), Tail::Missing);
}

#[test]
fn a_large_file_is_read_in_bounded_steps() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("t.jsonl");
    fs::write(&p, b"one\ntwo\nthree\n").unwrap();
    let mut t = Tailer::new();
    assert_eq!(t.read_at_most(&p, 5), lines(&[b"one"]));
    assert_eq!(t.read_at_most(&p, 5), lines(&[b"two"]));
    assert_eq!(t.read_at_most(&p, 0), lines(&[]));
    assert_eq!(t.read_at_most(&p, 100), lines(&[b"three"]));
    assert_eq!(t.read_at_most(&p, 100), Tail::NoChange);
}
