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

/// The parent, command name and start token of `pid` (panorama
/// `hooks._stat`): from `/proc/<pid>/stat`, or, where that does not exist (no
/// procfs), from `ps -o ppid=,lstart=,comm= -p <pid>`. `None` if unknown.
pub fn stat(pid: Pid) -> Option<ProcStat> {
    match fs::read(format!("{PROC}/{pid}/stat")) {
        Ok(raw) => parse_proc_stat(&raw),
        Err(e) if e.kind() == ErrorKind::NotFound => parse_ps_stat(&run_ps(&[
            "-o",
            "ppid=,lstart=,comm=",
            "-p",
            &pid.to_string(),
        ])?),
        Err(_) => None,
    }
}

/// `/proc/<pid>/stat`: the command name sits between the first `(` and the
/// last `)` (it may contain both); after it come state, ppid, …, and the 22nd
/// field, starttime. Not UTF-8 is unreadable, as in the Python.
fn parse_proc_stat(raw: &[u8]) -> Option<ProcStat> {
    let text = std::str::from_utf8(raw).ok()?;
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?;
    let fields: Vec<&str> = text.get(close + 2..)?.split_whitespace().collect();
    Some(ProcStat {
        ppid: fields.get(1)?.parse().ok()?,
        comm: comm.to_owned(),
        start: (*fields.get(19)?).to_owned(),
    })
}

/// One `ps -o ppid=,lstart=,comm=` line, split on whitespace: the ppid, the
/// command's base name from the last word, and every word between them as the
/// start token. This exact token is what panorama's hooks have recorded on
/// macOS, so it is kept word for word.
fn parse_ps_stat(out: &str) -> Option<ProcStat> {
    let words: Vec<&str> = out.split_whitespace().collect();
    let (first, last) = (words.first()?, words.last()?);
    let between = words.get(1..words.len().saturating_sub(1)).unwrap_or(&[]);
    Some(ProcStat {
        ppid: first.parse().ok()?,
        comm: basename(last).to_owned(),
        start: between.join(" "),
    })
}

/// The argv of `pid` from `/proc/<pid>/cmdline` (empty arguments dropped,
/// invalid UTF-8 replaced). `Err(NotFound)` when there is no such file.
fn proc_cmdline(proc: &str, pid: Pid) -> std::io::Result<Vec<String>> {
    let raw = fs::read(format!("{proc}/{pid}/cmdline"))?;
    Ok(raw
        .split(|&b| b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect())
}

/// Runs `ps` with `args` and returns its stdout whatever its exit status;
/// `None` if it cannot run or takes longer than [`PS_TIMEOUT`].
fn run_ps(args: &[&str]) -> Option<String> {
    let mut child = Command::new("ps")
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
    let deadline = Instant::now() + PS_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return None;
            }
        }
    }
    let out = reader.join().ok()?.ok()?;
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// The whole process table from `ps`, read once (inbox `proc._ps_table`): a
/// hook walks up to [`super::MAX_DEPTH`] ancestors, and a `ps` per ancestor
/// costs 10-30 ms each on the hottest path.
#[derive(Debug, Default)]
struct PsTable(BTreeMap<Pid, (ProcStat, Vec<String>)>);

impl PsTable {
    /// Two `ps` calls through `run`; empty when either fails.
    fn read(run: PsRunner) -> Self {
        let names = run(&["-A", "-o", "pid=,ppid=,lstart=,comm="]);
        let argvs = run(&["-A", "-o", "pid=,command="]);
        match (names, argvs) {
            (Some(names), Some(argvs)) => Self::parse(&names, &argvs),
            _ => Self::default(),
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

/// The `lstart` column is five words (`Sat Oct 17 18:26:02 2026`).
const LSTART_WORDS: usize = 5;

/// One `ps -A -o pid=,ppid=,lstart=,comm=` line. The command name is the base
/// name of the whole rest of the line (it may hold spaces; inbox), and the
/// start token is every word between the ppid and the last word, exactly as
/// [`parse_ps_stat`] reads the same columns for one pid.
fn parse_ps_row(line: &str) -> Option<(Pid, ProcStat)> {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() < 3 + LSTART_WORDS {
        return None;
    }
    let pid = words[0].parse().ok()?;
    let ppid = words[1].parse().ok()?;
    // the rest of the line after pid, ppid and lstart, inner spaces kept
    let mut rest = line.trim_start();
    for _ in 0..2 + LSTART_WORDS {
        let word_end = rest.find(char::is_whitespace)?;
        rest = rest[word_end..].trim_start();
    }
    let stat = ProcStat {
        ppid,
        comm: basename(rest.trim_end()).to_owned(),
        start: words[2..words.len() - 1].join(" "),
    };
    Some((pid, stat))
}

/// This machine's process table, for one walk up the tree. Reads procfs;
/// where there is none, it reads the whole table with `ps` once, on first use,
/// and keeps it for the life of the value. Make a new one per walk, so no walk
/// sees an old table.
#[derive(Debug)]
pub struct SystemProcesses {
    proc: &'static str,
    run: PsRunner,
    ps: OnceCell<PsTable>,
}

/// How `ps` is run (a test seam).
type PsRunner = fn(&[&str]) -> Option<String>;

impl Default for SystemProcesses {
    fn default() -> Self {
        Self {
            proc: PROC,
            run: run_ps,
            ps: OnceCell::new(),
        }
    }
}

impl SystemProcesses {
    /// A table for one walk.
    pub fn new() -> Self {
        Self::default()
    }

    fn ps(&self) -> &PsTable {
        self.ps.get_or_init(|| PsTable::read(self.run))
    }
}

impl ProcessTable for SystemProcesses {
    fn stat(&self, pid: Pid) -> Option<ProcStat> {
        match fs::read(format!("{}/{pid}/stat", self.proc)) {
            Ok(raw) => parse_proc_stat(&raw),
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
    Some(stat(id.pid)?.start == id.start)
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
    Some(out)
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
            if let Some(task) = target.to_str().and_then(task_of) {
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
        let st = parse_proc_stat(line).unwrap();
        assert_eq!(st.comm, "we(ird) n)ame");
        assert_eq!(st.ppid, 4100);
        assert_eq!(st.start, "987654");
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
        let st = parse_ps_stat("  812 Sat Oct  3 09:17:16 2026 /usr/local/bin/claude\n").unwrap();
        assert_eq!(st.ppid, 812);
        assert_eq!(st.comm, "claude");
        assert_eq!(st.start, "Sat Oct 3 09:17:16 2026"); // words joined by one space
        // a command path with a space: the Python's start token takes the first part, kept as is
        let st = parse_ps_stat("1 Sat Oct 17 18:26:02 2026 /Applications/My App/bin/node").unwrap();
        assert_eq!(st.comm, "node");
        assert_eq!(st.start, "Sat Oct 17 18:26:02 2026 /Applications/My");
        assert_eq!(parse_ps_stat(""), None);
        assert_eq!(parse_ps_stat("x Sat Oct 17 18:26:02 2026 sh"), None);
        let st = parse_ps_stat("7").unwrap();
        assert_eq!((st.ppid, st.comm.as_str(), st.start.as_str()), (7, "7", ""));
    }

    #[test]
    fn a_ps_table() {
        let names = "    1     0 Wed Oct 14 09:17:16 2026 /sbin/launchd\n\
                     812     1 Sat Oct  3 09:17:16 2026 /Applications/My App/claude\n\
                     900   812 Sat Oct 17 18:26:02 2026 zsh\n\
                     bad line\n\
                     901   900 Sat Oct 17 18:26:02 2026\n";
        let argvs = "  1 /sbin/launchd\n812 claude --resume\n900 -zsh\n";
        let t = PsTable::parse(names, argvs).0;
        assert_eq!(t.len(), 3);
        let (st, argv) = &t[&812];
        assert_eq!((st.ppid, st.comm.as_str()), (1, "claude"));
        assert_eq!(st.start, "Sat Oct 3 09:17:16 2026 /Applications/My");
        assert_eq!(argv, &["claude", "--resume"]);
        assert_eq!(
            parse_ps_stat("1 Sat Oct  3 09:17:16 2026 /Applications/My App/claude")
                .unwrap()
                .start,
            st.start,
            "the table and the one-pid read give one start token"
        );
        assert_eq!(t[&900].0.comm, "zsh");
        assert_eq!(t[&900].1, ["-zsh"]);
    }

    #[test]
    fn the_ps_table_and_procfs_agree_on_this_process() {
        let me = std::process::id();
        let table = PsTable::read(run_ps).0;
        let (st, argv) = table.get(&me).expect("ps lists this process");
        assert_eq!(st.ppid, parent_pid());
        assert!(!argv.is_empty());
        // and the one-pid ps read gives the same start token as the table
        let one =
            parse_ps_stat(&run_ps(&["-o", "ppid=,lstart=,comm=", "-p", &me.to_string()]).unwrap())
                .unwrap();
        assert_eq!(one.start, st.start);
        assert_eq!(one.ppid, st.ppid);
    }

    thread_local! {
        static PS_CALLS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// inbox `test_without_procfs_a_walk_reads_the_process_table_once`'s chain.
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
        let out = if args.contains(&"pid=,ppid=,lstart=,comm=") {
            chain
                .iter()
                .map(|(p, pp, comm, _)| format!("{p:>6} {pp:>6} Sat Oct  3 09:17:16 2026 {comm}\n"))
                .collect()
        } else {
            chain
                .iter()
                .map(|(p, _, _, cmd)| format!("{p:>6} {cmd}\n"))
                .collect()
        };
        Some(out)
    }

    fn no_procfs() -> SystemProcesses {
        SystemProcesses {
            proc: "/nonexistent-procfs",
            run: fake_ps,
            ps: OnceCell::new(),
        }
    }

    /// inbox `test_without_procfs_a_walk_reads_the_process_table_once`: two
    /// `ps` calls per walk, whatever its depth; a new walk reads a fresh table.
    #[test]
    fn test_without_procfs_a_walk_reads_the_process_table_once() {
        let calls = || PS_CALLS.with(|c| c.borrow().len());
        assert!(super::super::nested_claude(&no_procfs(), 100));
        assert_eq!(calls(), 2);
        assert_eq!(super::super::owning_claude(&no_procfs(), 100), Some(102));
        assert_eq!(calls(), 4);
        let table = no_procfs();
        let st = table.stat(104).unwrap();
        assert_eq!((st.ppid, st.comm.as_str()), (105, "claude"));
        assert!(table.cmdline(102)[0].ends_with("/2.1.301"));
        assert_eq!(calls(), 6);
        assert_eq!(table.stat(999), None);
        assert_eq!(calls(), 6);
    }

    #[test]
    fn this_process_reads_back() {
        let me = std::process::id();
        let st = stat(me).expect("own stat");
        assert_eq!(st.ppid, parent_pid());
        let table = SystemProcesses::new();
        assert_eq!(table.stat(me), Some(st));
        assert!(!table.cmdline(me).is_empty());
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
        let start = stat(pid).map(|s| s.start).unwrap_or_default();
        child.wait().unwrap();
        // reaped: no process has this pid (until it is reused, which a start token catches)
        assert_eq!(
            process_alive(&ProcessIdentity::new(pid, start, None)),
            Some(false)
        );
        assert!(!pid_alive(Some(pid)));
        let me = std::process::id();
        let mine = stat(me).unwrap().start;
        assert_eq!(
            process_alive(&ProcessIdentity::new(
                me,
                mine.clone(),
                Some("pid:[1]".into())
            )),
            None
        );
        assert_eq!(
            process_alive(&ProcessIdentity::new(me, mine.clone(), own_pidns())),
            Some(true)
        );
        assert!(pid_alive(Some(me)));
        assert!(pid_alive(None));
        assert!(pid_alive(Some(0)));
    }

    #[test]
    fn a_process_we_may_not_signal_is_alive_by_its_start_token() {
        // pid 1 belongs to root: kill says EPERM (unless the tests run as root), the stat decides
        let Some(init) = stat(1) else { return };
        assert_eq!(
            process_alive(&ProcessIdentity::new(1, init.start.clone(), None)),
            Some(true)
        );
        assert_eq!(
            process_alive(&ProcessIdentity::new(1, format!("{}x", init.start), None)),
            Some(false)
        );
        assert!(pid_alive(Some(1)));
    }
}
