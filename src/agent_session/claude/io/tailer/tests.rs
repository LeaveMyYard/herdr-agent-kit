use super::*;

#[test]
fn partial_line_is_buffered_until_its_newline() {
    let mut t = Tailer::new();
    assert_eq!(
        t.consume(b"{\"a\":1}\n{\"b\":"),
        vec![b"{\"a\":1}".to_vec()]
    );
    assert_eq!(t.consume(b"2}\r\n\n  \n"), vec![b"{\"b\":2}".to_vec()]);
}

#[test]
fn runaway_line_is_dropped_and_reading_resyncs() {
    let mut t = Tailer::new();
    let big = vec![b'x'; MAX_PARTIAL + 1];
    assert!(t.consume(&big).is_empty());
    assert!(t.consume(b"xx").is_empty());
    assert_eq!(t.consume(b"yyy\nok\n"), vec![b"ok".to_vec()]);
}

#[test]
fn a_partial_line_at_the_cap_is_kept() {
    let mut t = Tailer::new();
    assert!(t.consume(&vec![b'x'; MAX_PARTIAL]).is_empty());
    assert_eq!(t.consume(b"\n"), vec![vec![b'x'; MAX_PARTIAL]]);
}
