use super::*;
use crate::agent_procs::claude;

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
    assert!(claude::nested_claude(&no_procfs(fake_ps), 100));
    assert_eq!(ps_calls() - before, 2);
    assert_eq!(claude::owning_claude(&no_procfs(fake_ps), 100), Some(102));
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
        claude::claude_process(&no_procfs(fake_ps), 100),
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
    assert!(!claude::nested_claude(&no_procfs(first_fails), 100));
    assert_eq!(ps_calls() - before, 1);
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
