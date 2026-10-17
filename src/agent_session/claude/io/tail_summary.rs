//! [`summarize_tail`]: activity, calls in flight, last text and model from
//! the end of a transcript.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::agent_session::claude::wire::TailInfo;

/// The tail window [`summarize_tail`] starts with.
pub const TAIL_BYTES: u64 = 64 * 1024;
/// The largest tail window: it doubles up to this when the last records are
/// huge.
pub const TAIL_MAX: u64 = 8 * 1024 * 1024;
/// The first line is read for its time only up to this many bytes.
pub const HEAD_MAX: u64 = 256 * 1024;

/// Activity, calls in flight, last text, model and times from the end of a
/// transcript (and its first line): panorama's `summarize_tail`. The window
/// starts at [`TAIL_BYTES`] and doubles, up to [`TAIL_MAX`], until it holds at
/// least two `user`/`assistant` records and its first newline is in its first
/// half, so one huge last record cannot hide them. A file that cannot be read
/// gives an empty summary.
pub fn summarize_tail(path: &Path, cwd: Option<&str>) -> TailInfo {
    summarize_tail_from(path, cwd, TAIL_BYTES)
}

/// [`summarize_tail`] starting from another window size (at least one byte).
pub fn summarize_tail_from(path: &Path, cwd: Option<&str>, window: u64) -> TailInfo {
    match read_tail(path, window.max(1)) {
        Ok((head, data, start)) => {
            let mut lines: Vec<&[u8]> = data.split(|&c| c == b'\n').collect();
            if start > 0 {
                lines.remove(0);
            }
            TailInfo::from_lines(head.as_deref(), &lines, cwd)
        }
        Err(_) => TailInfo::default(),
    }
}

fn count(data: &[u8], needle: &[u8]) -> usize {
    memchr::memmem::find_iter(data, needle).count()
}

type TailRead = (Option<Vec<u8>>, Vec<u8>, u64);

fn read_tail(path: &Path, mut window: u64) -> std::io::Result<TailRead> {
    let size = fs::metadata(path)?.len();
    let mut file = File::open(path)?;
    let mut head = Vec::new();
    BufReader::new((&mut file).take(HEAD_MAX)).read_until(b'\n', &mut head)?;
    let head = head.ends_with(b"\n").then_some(head);
    loop {
        let start = size.saturating_sub(window);
        file.seek(SeekFrom::Start(start))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let dated = count(&data, br#""type":"assistant""#) + count(&data, br#""type":"user""#);
        let first_newline = memchr::memchr(b'\n', &data).map_or(-1, |i| i as i64);
        if start == 0
            || window >= TAIL_MAX
            || (dated >= 2 && first_newline < (data.len() / 2) as i64)
        {
            return Ok((head, data, start));
        }
        window *= 2;
    }
}
