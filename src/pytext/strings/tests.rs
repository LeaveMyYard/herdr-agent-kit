use super::*;

#[test]
fn splitlines_matches_python() {
    assert_eq!(
        py_splitlines("a\r\nb\rc\nd\x0be\u{2028}f"),
        vec!["a", "b", "c", "d", "e", "f"]
    );
    assert_eq!(py_splitlines("a\n"), vec!["a"]);
    assert_eq!(py_splitlines(""), Vec::<&str>::new());
    assert_eq!(py_splitlines("\n"), vec![""]);
    assert_eq!(
        py_splitlines("a\r\r\nb\u{85}c\x1cd"),
        vec!["a", "", "b", "c", "d"]
    );
}

#[test]
fn strip_and_blank() {
    assert_eq!(py_strip("\u{1c} a b\u{3000}\n"), "a b");
    assert!(blank(b" \t\r\n\x0b\x0c"));
    assert!(blank(b""));
    // bytes.strip() strips ASCII whitespace only, and not \x1c
    assert!(!blank(b"\x1c"));
    assert!(!blank(" \u{a0}".as_bytes()));
}

#[test]
fn digits_and_ids() {
    assert_eq!(ascii_digits("007x"), "007");
    assert_eq!(ascii_digits("x7"), "");
    assert_eq!(parse_id("007"), Some(7));
    assert_eq!(parse_id("99999999999999999999999"), None);
    #[cfg(feature = "internal")]
    assert_eq!(py_len("\u{1F680}a"), 2);
}
