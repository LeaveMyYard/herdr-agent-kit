use super::*;

#[test]
fn the_space_sets_differ_where_the_languages_do() {
    for c in ['\u{1c}', '\u{1f}', '\u{85}'] {
        assert!(py_space(c) && !js_space(c), "{c:?}");
    }
    assert!(js_space('\u{feff}') && !py_space('\u{feff}'));
    assert!(!'\u{1c}'.is_whitespace(), "why py_space exists");
    for c in [' ', '\t', '\u{a0}', '\u{3000}', '\u{2028}'] {
        assert!(py_space(c) && js_space(c), "{c:?}");
    }
    for c in ['a', '\u{200b}', '\u{180e}', '\0'] {
        assert!(!py_space(c) && !js_space(c), "{c:?}");
    }
}

#[test]
fn digits_and_ids() {
    assert_eq!(ascii_digits("007x"), "007");
    assert_eq!(ascii_digits("x7"), "");
    assert_eq!(parse_id("007"), Some(7));
    assert_eq!(parse_id("99999999999999999999999"), None);
    assert_eq!(py_len("\u{1F680}a"), 2);
    assert_eq!(digits_then_space("12 x"), Some("12"));
    assert_eq!(
        digits_then_space("99999999999999999999999 x"),
        Some("99999999999999999999999")
    );
    for s in ["12x", "12", " 12 ", "x 1"] {
        assert_eq!(digits_then_space(s), None, "{s:?}");
    }
}
