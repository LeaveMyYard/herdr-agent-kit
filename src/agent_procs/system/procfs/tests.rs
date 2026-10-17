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
