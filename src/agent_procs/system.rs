//! This machine's process table: procfs where there is one, `ps` elsewhere
//! (macOS). Read-only.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{ErrorKind, Read};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{fs, thread};

use super::{Pid, ProcStat, ProcessIdentity, ProcessTable, basename};

const PROC: &str = "/proc";
/// A runaway process tree never makes one liveness pass slow.
const MAX_PROCS: usize = 4096;
/// How long one `ps` call may take.
const PS_TIMEOUT: Duration = Duration::from_secs(1);

/// The pid of this process's parent (a hook's parent is the shell or Claude
/// that ran it): where the walks start.
pub fn parent_pid() -> Pid {
    std::os::unix::process::parent_id()
}

/// The start token of `pid` (panorama `hooks._stat`'s third value): from
/// `/proc/<pid>/stat`, or, where that does not exist (no procfs), from
/// `ps -o ppid=,lstart=,comm= -p <pid>`. `None` if unknown. Panorama's hooks
/// write this token into their records, so its format is a wire format.
pub fn start_token(pid: Pid) -> Option<String> {
    SystemProcesses::new().start(pid)
}

/// `/proc/<pid>/stat`: the command name sits between the first `(` and the
/// last `)` (it may contain both); after it come state, ppid, …, and the 22nd
/// field, starttime (the start token). Not UTF-8 is unreadable, as in the
/// Python.
fn parse_proc_stat(raw: &[u8]) -> Option<(ProcStat, String)> {
    let text = std::str::from_utf8(raw).ok()?;
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?;
    let fields: Vec<&str> = text.get(close + 2..)?.split_whitespace().collect();
    let stat = ProcStat {
        ppid: fields.get(1)?.parse().ok()?,
        comm: comm.to_owned(),
    };
    Some((stat, (*fields.get(19)?).to_owned()))
}

/// One `ps -o ppid=,lstart=,comm=` line, split on whitespace: the ppid, the
/// command's base name from the last word, and every word between them as the
/// start token (however many words `lstart` has in the user's locale). This
/// exact token is what panorama's hooks have recorded on macOS, so it is kept
/// word for word.
fn parse_ps_stat(out: &str) -> Option<(ProcStat, String)> {
    let words: Vec<&str> = out.split_whitespace().collect();
    let (first, last) = (words.first()?, words.last()?);
    let between = words.get(1..words.len().saturating_sub(1)).unwrap_or(&[]);
    let stat = ProcStat {
        ppid: first.parse().ok()?,
        comm: basename(last).to_owned(),
    };
    Some((stat, between.join(" ")))
}

/// The argv of `pid` from `<proc>/<pid>/cmdline` (empty arguments dropped,
/// invalid UTF-8 replaced). `Err(NotFound)` when there is no such file.
fn proc_cmdline(proc: &str, pid: Pid) -> std::io::Result<Vec<String>> {
    let raw = fs::read(format!("{proc}/{pid}/cmdline"))?;
    Ok(raw
        .split(|&b| b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect())
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
fn run_ps(args: &[&str]) -> Option<String> {
    run_with_timeout("ps", args, PS_TIMEOUT)
}

/// The whole process table from `ps`, read once (inbox `proc._ps_table`): a
/// hook walks up to [`super::MAX_DEPTH`] ancestors, and a `ps` per ancestor
/// costs 10-30 ms each on the hottest path.
#[derive(Debug, Default)]
struct PsTable(BTreeMap<Pid, (ProcStat, Vec<String>)>);

impl PsTable {
    /// Two `ps` calls through `run` (the second only if the first worked);
    /// empty when either fails.
    fn read(run: PsRunner) -> Self {
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

/// This machine's process table, for one walk up the tree. Reads procfs;
/// where there is none, it reads the whole table with `ps` once, on first use,
/// and keeps it for the life of the value: make a new one per walk (the
/// `*_here` functions do), so no walk sees an old table.
#[derive(Debug)]
pub struct SystemProcesses {
    proc: &'static str,
    run: PsRunner,
    ps: OnceCell<PsTable>,
}

/// How `ps` is run (a test seam).
type PsRunner = fn(&[&str]) -> Option<String>;

impl SystemProcesses {
    /// A table for one walk.
    pub fn new() -> Self {
        Self {
            proc: PROC,
            run: run_ps,
            ps: OnceCell::new(),
        }
    }

    fn ps(&self) -> &PsTable {
        self.ps.get_or_init(|| PsTable::read(self.run))
    }

    fn proc_stat(&self, pid: Pid) -> std::io::Result<Option<(ProcStat, String)>> {
        Ok(parse_proc_stat(&fs::read(format!(
            "{}/{pid}/stat",
            self.proc
        ))?))
    }
}

impl Default for SystemProcesses {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTable for SystemProcesses {
    fn stat(&self, pid: Pid) -> Option<ProcStat> {
        match self.proc_stat(pid) {
            Ok(st) => st.map(|(st, _)| st),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                self.ps().0.get(&pid).map(|(st, _)| st.clone())
            }
            Err(_) => None,
        }
    }

    fn cmdline(&self, pid: Pid) -> Vec<String> {
        match proc_cmdline(self.proc, pid) {
            Ok(argv) => argv,
            Err(e) if e.kind() == ErrorKind::NotFound => self
                .ps()
                .0
                .get(&pid)
                .map(|(_, argv)| argv.clone())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// procfs, or the one-pid `ps` read (panorama `_stat`), never the walk's
    /// table: the token must be byte-identical to what the Python hooks wrote.
    fn start(&self, pid: Pid) -> Option<String> {
        match self.proc_stat(pid) {
            Ok(st) => st.map(|(_, start)| start),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                let pid = pid.to_string();
                let out = (self.run)(&["-o", "ppid=,lstart=,comm=", "-p", &pid])?;
                parse_ps_stat(&out).map(|(_, start)| start)
            }
            Err(_) => None,
        }
    }
}

/// This process's pid namespace (`readlink /proc/self/ns/pid`, such as
/// `pid:[4026531836]`), or `None` where it cannot be read (macOS).
pub fn own_pidns() -> Option<String> {
    fs::read_link(format!("{PROC}/self/ns/pid"))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// What `kill(pid, 0)` says.
enum Signal0 {
    Delivered,
    NoSuchProcess,
    NotPermitted,
    Failed,
}

fn signal0(pid: Pid) -> Signal0 {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return Signal0::Failed;
    };
    // SAFETY: signal 0 sends nothing; kill only checks that `pid` exists and
    // may be signalled.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Signal0::Delivered;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Signal0::NoSuchProcess,
        Some(libc::EPERM) => Signal0::NotPermitted,
        _ => Signal0::Failed,
    }
}

/// Is the recorded process still running (panorama `hooks.process_alive`)?
///
/// - `None` when it was recorded in another pid namespace than ours, when the
///   check fails, or when its start token cannot be read.
/// - `Some(false)` when no process has its pid, or one does with another
///   start token (the pid was reused).
/// - `Some(true)` when its pid exists with its start token, even if we may not
///   signal it.
pub fn process_alive(id: &ProcessIdentity) -> Option<bool> {
    if let Some(ns) = &id.pidns
        && own_pidns().as_deref() != Some(ns.as_str())
    {
        return None;
    }
    match signal0(id.pid) {
        Signal0::NoSuchProcess => return Some(false),
        Signal0::Failed => return None,
        Signal0::Delivered | Signal0::NotPermitted => {}
    }
    Some(start_token(id.pid)? == id.start)
}

/// Is `pid` alive, by `kill(pid, 0)` alone (inbox `proc.alive`)? An unknown
/// pid (`None` or 0) and a process we may not signal count as alive: the
/// inbox's other bounds (supersede, max age) still apply to them.
pub fn pid_alive(pid: Option<Pid>) -> bool {
    match pid {
        None | Some(0) => true,
        Some(pid) => !matches!(signal0(pid), Signal0::NoSuchProcess),
    }
}

fn children(pid: Pid) -> Vec<Pid> {
    let mut out = Vec::new();
    let Ok(tids) = fs::read_dir(format!("{PROC}/{pid}/task")) else {
        return out;
    };
    for tid in tids.flatten() {
        let path = tid.path().join("children");
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        // one bad word drops that thread's list, as in the Python
        if let Ok(kids) = text
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<Vec<Pid>, _>>()
        {
            out.extend(kids);
        }
    }
    out
}

/// Every process below `pid`, ascending, at most 4096 of them (panorama
/// `bgproc.descendants`); `None` when `pid` cannot be read (gone, or no
/// procfs).
pub fn descendants(pid: Pid) -> Option<Vec<Pid>> {
    if !fs::metadata(format!("{PROC}/{pid}/task")).is_ok_and(|m| m.is_dir()) {
        return None;
    }
    Some(below(pid, children))
}

/// Every pid below `pid` by `children`, ascending, at most [`MAX_PROCS`].
fn below(pid: Pid, children: impl Fn(Pid) -> Vec<Pid>) -> Vec<Pid> {
    let mut seen = HashSet::new();
    let mut stack = children(pid);
    while seen.len() < MAX_PROCS {
        let Some(p) = stack.pop() else { break };
        if seen.insert(p) {
            stack.extend(children(p));
        }
    }
    let mut out: Vec<Pid> = seen.into_iter().collect();
    out.sort_unstable();
    out
}

/// The task id of a background task's output file, `…/tasks/<id>.output`
/// (panorama `bgproc.task_of`).
pub fn task_of(path: &str) -> Option<&str> {
    let (head, name) = path.rsplit_once('/')?;
    let head = match head.trim_end_matches('/') {
        "" => head,
        trimmed => trimmed,
    };
    if basename(head) != "tasks" {
        return None;
    }
    name.strip_suffix(".output").filter(|id| !id.is_empty())
}

/// The ids of the background tasks whose output file a descendant of `pid`
/// holds open (panorama `bgproc.live_tasks`): Claude Code runs a background
/// Bash as a shell below its own process, writing to
/// `<tmp>/claude-<uid>/<project>/<session>/tasks/<task id>.output`, and the
/// shell (or a child) holds that file open while it lives. `None` when unknown
/// (no procfs, `pid` unreadable): never "dead".
///
/// A shell left over from an earlier Claude process (the session was resumed)
/// is not a descendant of the current one, so it does not count.
pub fn live_tasks(pid: Pid) -> Option<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for p in descendants(pid)? {
        let Ok(fds) = fs::read_dir(format!("{PROC}/{p}/fd")) else {
            continue; // exited meanwhile, or not ours to read
        };
        for fd in fds.flatten() {
            let Ok(target) = fs::read_link(fd.path()) else {
                continue;
            };
            // lossy, as Python's surrogate escapes: the `tasks/<id>.output` tail is ASCII
            if let Some(task) = task_of(&target.to_string_lossy()) {
                out.insert(task.to_owned());
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proc_stat_line_with_parentheses_in_the_name() {
        let line = b"4242 (we(ird) n)ame) S 4100 4242 4100 34816 4242 4194304 1 2 3 4 5 6 7 8 20 0 1 0 987654 1000 200";
        let (st, start) = parse_proc_stat(line).unwrap();
        assert_eq!(st.comm, "we(ird) n)ame");
        assert_eq!(st.ppid, 4100);
        assert_eq!(start, "987654");
        assert_eq!(parse_proc_stat(b"4242 (x) S 4100"), None); // no starttime
        assert_eq!(parse_proc_stat(b"4242 (x)"), None);
        assert_eq!(parse_proc_stat(b"4242 x S 1"), None);
        assert_eq!(
            parse_proc_stat(b"4242 (\xff) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20"),
            None
        );
    }

    #[test]
    fn a_ps_line_for_one_pid() {
        let (st, start) =
            parse_ps_stat("  812 Sat Oct  3 09:17:16 2026 /usr/local/bin/claude\n").unwrap();
        assert_eq!((st.ppid, st.comm.as_str()), (812, "claude"));
        assert_eq!(start, "Sat Oct 3 09:17:16 2026"); // words joined by one space
        // a command path with a space: the Python's start token takes the first part, kept as is
        let (st, start) =
            parse_ps_stat("1 Sat Oct 17 18:26:02 2026 /Applications/My App/bin/node").unwrap();
        assert_eq!(st.comm, "node");
        assert_eq!(start, "Sat Oct 17 18:26:02 2026 /Applications/My");
        // lstart in a locale with 4 or 6 words: every word between ppid and comm
        assert_eq!(
            parse_ps_stat("1 2026/10/17 18:26:02 Sat 2026 claude")
                .unwrap()
                .1,
            "2026/10/17 18:26:02 Sat 2026"
        );
        assert_eq!(parse_ps_stat("").map(|x| x.1), None);
        assert_eq!(parse_ps_stat("x Sat Oct 17 18:26:02 2026 sh"), None);
        let (st, start) = parse_ps_stat("7").unwrap();
        assert_eq!((st.ppid, st.comm.as_str(), start.as_str()), (7, "7", ""));
    }

    #[test]
    fn a_ps_table() {
        let names = "    1     0 /sbin/launchd\n\
                     812     1 /Applications/My App/claude\n\
                     900   812 -zsh\n\
                     bad line\n\
                     901   900\n\
                     902   900   \n";
        let argvs = "  1 /sbin/launchd\n812 claude --resume\n900 -zsh\n";
        let t = PsTable::parse(names, argvs).0;
        assert_eq!(t.keys().copied().collect::<Vec<_>>(), [1, 812, 900]);
        let (st, argv) = &t[&812];
        assert_eq!((st.ppid, st.comm.as_str()), (1, "claude"));
        assert_eq!(argv, &["claude", "--resume"]);
        assert_eq!(t[&900].0.comm, "-zsh");
        assert_eq!(t[&900].1, ["-zsh"]);
        // inbox's rule: the base name of the whole rest of the line, so a
        // space in the program's own file name stays in the name (panorama's
        // `_stat` took the last word only: "claude" here)
        let t = PsTable::parse("  77     1 /opt/my claude\n", "").0;
        assert_eq!(t[&77].0.comm, "my claude");
    }

    #[test]
    fn the_ps_table_and_procfs_agree_on_this_process() {
        let me = std::process::id();
        let table = PsTable::read(run_ps).0;
        let (st, argv) = table.get(&me).expect("ps lists this process");
        assert_eq!(st.ppid, parent_pid());
        assert!(!argv.is_empty());
    }

    thread_local! {
        static PS_CALLS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// inbox `test_without_procfs_a_walk_reads_the_process_table_once`'s
    /// chain; the one-pid `lstart` read answers in a 4-word locale.
    fn fake_ps(args: &[&str]) -> Option<String> {
        let chain: [(Pid, Pid, &str, &str); 6] = [
            (100, 101, "python3", "python3 -I hooks.py"),
            (101, 102, "zsh", "zsh"),
            (
                102,
                103,
                "2.1.301",
                "/Users/u/.local/share/claude/versions/2.1.301 -p",
            ),
            (103, 104, "bash", "bash -c x"),
            // a command name with a space: only the comm column (the rest of its line) names it
            (
                104,
                105,
                "/Users/u/My Tools/claude",
                "/Users/u/My Tools/claude",
            ),
            (105, 1, "/bin/zsh", "-zsh"),
        ];
        PS_CALLS.with(|c| c.borrow_mut().push(args.join(" ")));
        let out = match args {
            ["-A", "-o", "pid=,ppid=,comm="] => chain
                .iter()
                .map(|(p, pp, comm, _)| format!("{p:>6} {pp:>6} {comm}\n"))
                .collect(),
            ["-A", "-o", "pid=,command="] => chain
                .iter()
                .map(|(p, _, _, cmd)| format!("{p:>6} {cmd}\n"))
                .collect(),
            ["-o", "ppid=,lstart=,comm=", "-p", pid] => chain
                .iter()
                .filter(|(p, ..)| p.to_string() == *pid)
                .map(|(_, pp, comm, _)| format!("{pp:>6} 2026/10/03 9:17:16 Sat {comm}\n"))
                .collect(),
            _ => return None,
        };
        Some(out)
    }

    fn no_procfs(run: PsRunner) -> SystemProcesses {
        SystemProcesses {
            proc: "/nonexistent-procfs",
            run,
            ps: OnceCell::new(),
        }
    }

    fn ps_calls() -> usize {
        PS_CALLS.with(|c| c.borrow().len())
    }

    /// inbox `test_without_procfs_a_walk_reads_the_process_table_once`: two
    /// `ps` calls per walk, whatever its depth; a new walk reads a fresh table.
    #[test]
    fn test_without_procfs_a_walk_reads_the_process_table_once() {
        let before = ps_calls();
        assert!(super::super::claude::nested_claude(
            &no_procfs(fake_ps),
            100
        ));
        assert_eq!(ps_calls() - before, 2);
        assert_eq!(
            super::super::claude::owning_claude(&no_procfs(fake_ps), 100),
            Some(102)
        );
        assert_eq!(ps_calls() - before, 4);
        let table = no_procfs(fake_ps);
        let st = table.stat(104).unwrap();
        assert_eq!((st.ppid, st.comm.as_str()), (105, "claude"));
        assert!(table.cmdline(102)[0].ends_with("/2.1.301"));
        assert_eq!(ps_calls() - before, 6);
        assert_eq!(table.stat(999), None);
        assert_eq!(ps_calls() - before, 6);
    }

    /// The found Claude's start token comes from the one-pid `ps` read
    /// (panorama `_stat`), whatever the locale's `lstart` looks like.
    #[test]
    fn without_procfs_the_claude_token_is_the_one_pid_read() {
        assert_eq!(
            super::super::claude::claude_process(&no_procfs(fake_ps), 100),
            Some((102, "2026/10/03 9:17:16 Sat".to_owned()))
        );
        assert_eq!(no_procfs(fake_ps).start(999), None);
    }

    /// inbox `_ps_table`: when the first `ps` fails the second is not run.
    #[test]
    fn a_failed_ps_skips_the_second_call() {
        fn first_fails(args: &[&str]) -> Option<String> {
            PS_CALLS.with(|c| c.borrow_mut().push(args.join(" ")));
            None
        }
        let before = ps_calls();
        assert!(!super::super::claude::nested_claude(
            &no_procfs(first_fails),
            100
        ));
        assert_eq!(ps_calls() - before, 1);
    }

    #[test]
    fn a_slow_command_is_killed_at_the_timeout() {
        let t = Instant::now();
        let out = run_with_timeout(
            "sh",
            &["-c", "echo started; exec sleep 5"],
            Duration::from_millis(200),
        );
        assert_eq!(out, None);
        assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
        assert_eq!(
            run_with_timeout("sh", &["-c", "echo ok"], Duration::from_secs(5)).as_deref(),
            Some("ok\n")
        );
        assert_eq!(
            run_with_timeout("/nonexistent/ps", &[], Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn this_process_reads_back() {
        let me = std::process::id();
        let table = SystemProcesses::new();
        let st = table.stat(me).expect("own stat");
        assert_eq!(st.ppid, parent_pid());
        assert!(!table.cmdline(me).is_empty());
        assert_eq!(table.start(me), start_token(me));
        assert!(start_token(me).is_some());
    }

    #[test]
    fn a_runaway_tree_is_cut_at_max_procs() {
        // a shell that forked 10 000 children
        let all = below(1, |p| {
            if p == 1 {
                (2..10_002).collect()
            } else {
                vec![]
            }
        });
        assert_eq!(all.len(), MAX_PROCS);
        let small = below(1, |p| {
            if p < 4 {
                vec![p * 2, p * 2 + 1]
            } else {
                vec![]
            }
        });
        assert_eq!(small, [2, 3, 4, 5, 6, 7]);
        // a cycle is visited once
        assert_eq!(below(1, |p| vec![p % 3 + 1]), [1, 2, 3]);
    }

    #[test]
    fn task_ids_from_output_paths() {
        assert_eq!(
            task_of("/tmp/claude-1000/p/s/tasks/b1x2.output"),
            Some("b1x2")
        );
        assert_eq!(
            task_of("/tmp/claude-1000/p/s/tasks//b1x2.output"),
            Some("b1x2")
        );
        assert_eq!(task_of("tasks/b1.output"), Some("b1"));
        assert_eq!(task_of("/x/tasks/.output"), None);
        assert_eq!(task_of("/x/tasks/b1.output (deleted)"), None);
        assert_eq!(task_of("/x/task/b1.output"), None);
        assert_eq!(task_of("/x/tasks/b1.out"), None);
        assert_eq!(task_of("b1.output"), None);
    }

    #[test]
    fn a_gone_process_is_dead_and_another_namespace_is_unknown() {
        let mut child = Command::new("true").spawn().unwrap();
        let pid = child.id();
        let start = start_token(pid).unwrap_or_default();
        child.wait().unwrap();
        // reaped: no process has this pid (until it is reused, which a start token catches)
        assert_eq!(
            process_alive(&ProcessIdentity::new(pid, start, None)),
            Some(false)
        );
        assert!(!pid_alive(Some(pid)));
        let me = std::process::id();
        let mine = start_token(me).unwrap();
        assert_eq!(
            process_alive(&ProcessIdentity::new(
                me,
                mine.clone(),
                Some("pid:[1]".into())
            )),
            None
        );
        assert_eq!(
            process_alive(&ProcessIdentity::new(me, mine, own_pidns())),
            Some(true)
        );
        assert!(pid_alive(Some(me)));
        assert!(pid_alive(None));
        assert!(pid_alive(Some(0)));
    }

    #[test]
    fn a_process_we_may_not_signal_is_alive_by_its_start_token() {
        // pid 1 belongs to root: kill says EPERM (unless the tests run as root), the token decides
        let Some(init) = start_token(1) else { return };
        assert_eq!(
            process_alive(&ProcessIdentity::new(1, init.clone(), None)),
            Some(true)
        );
        assert_eq!(
            process_alive(&ProcessIdentity::new(1, format!("{init}x"), None)),
            Some(false)
        );
        assert!(pid_alive(Some(1)));
    }
}
