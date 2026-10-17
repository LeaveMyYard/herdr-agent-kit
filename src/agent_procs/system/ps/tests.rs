use super::*;
use crate::agent_procs::parent_pid;

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
