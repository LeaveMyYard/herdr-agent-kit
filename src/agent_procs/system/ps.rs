//! `ps`, where there is no procfs (macOS): the whole table, read once per
//! walk, and the one-pid read of a start token.

use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::agent_procs::{Pid, ProcStat, basename};

/// How long one `ps` call may take.
const PS_TIMEOUT: Duration = Duration::from_secs(1);

/// How `ps` is run (a test seam).
pub(super) type PsRunner = fn(&[&str]) -> Option<String>;

/// One `ps -o ppid=,lstart=,comm=` line, split on whitespace: the ppid, the
/// command's base name from the last word, and every word between them as the
/// start token (however many words `lstart` has in the user's locale). This
/// exact token is what panorama's hooks have recorded on macOS, so it is kept
/// word for word.
pub(super) fn parse_ps_stat(out: &str) -> Option<(ProcStat, String)> {
    let words: Vec<&str> = out.split_whitespace().collect();
    let (first, last) = (words.first()?, words.last()?);
    let between = words.get(1..words.len().saturating_sub(1)).unwrap_or(&[]);
    let stat = ProcStat {
        ppid: first.parse().ok()?,
        comm: basename(last).to_owned(),
    };
    Some((stat, between.join(" ")))
}

/// Runs `program` with `args` and returns its stdout whatever its exit
/// status; `None` if it cannot run or takes longer than `timeout` (then it is
/// killed and reaped).
fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // read on a thread: `ps -A` can print more than a pipe holds
    let reader = thread::spawn(move || {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).map(|_| out)
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                // a grandchild may still hold the pipe: do not wait for the reader
                return None;
            }
        }
    }
    // the program exited: its stdout is closed unless a grandchild still holds
    // it (`ps` starts none), so this join does not wait for long
    let out = reader.join().ok()?.ok()?;
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// `ps` with `args`, bounded by [`PS_TIMEOUT`].
pub(super) fn run_ps(args: &[&str]) -> Option<String> {
    run_with_timeout("ps", args, PS_TIMEOUT)
}

/// The whole process table from `ps`, read once (inbox `proc._ps_table`): a
/// hook walks up to [`MAX_DEPTH`](crate::agent_procs::MAX_DEPTH) ancestors,
/// and a `ps` per ancestor costs 10-30 ms each on the hottest path.
#[derive(Debug, Default)]
pub(super) struct PsTable(pub(super) BTreeMap<Pid, (ProcStat, Vec<String>)>);

impl PsTable {
    /// Two `ps` calls through `run` (the second only if the first worked);
    /// empty when either fails.
    pub(super) fn read(run: PsRunner) -> Self {
        let Some(names) = run(&["-A", "-o", "pid=,ppid=,comm="]) else {
            return Self::default();
        };
        match run(&["-A", "-o", "pid=,command="]) {
            Some(argvs) => Self::parse(&names, &argvs),
            None => Self::default(),
        }
    }

    fn parse(names: &str, argvs: &str) -> Self {
        let mut args: BTreeMap<Pid, Vec<String>> = BTreeMap::new();
        for line in argvs.lines() {
            let mut words = line.split_whitespace();
            if let Some(Ok(pid)) = words.next().map(str::parse::<Pid>) {
                args.insert(pid, words.map(str::to_owned).collect());
            }
        }
        let mut table = BTreeMap::new();
        for line in names.lines() {
            let Some((pid, stat)) = parse_ps_row(line) else {
                continue;
            };
            let argv = args.remove(&pid).unwrap_or_default();
            table.insert(pid, (stat, argv));
        }
        Self(table)
    }
}

/// One `ps -A -o pid=,ppid=,comm=` line (inbox `_ps_table`): pid, ppid, and
/// the base name of the whole rest of the line as the command name (it may
/// hold spaces).
fn parse_ps_row(line: &str) -> Option<(Pid, ProcStat)> {
    let mut rest = line.trim_start();
    let mut field = || {
        let end = rest.find(char::is_whitespace)?;
        let word = &rest[..end];
        rest = rest[end..].trim_start();
        Some(word)
    };
    let pid = field()?.parse().ok()?;
    let ppid = field()?.parse().ok()?;
    let comm = rest.trim_end();
    if comm.is_empty() {
        return None;
    }
    let stat = ProcStat {
        ppid,
        comm: basename(comm).to_owned(),
    };
    Some((pid, stat))
}

#[cfg(test)]
mod tests;
