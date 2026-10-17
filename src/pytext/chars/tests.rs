use super::*;

/// The explicit `str.isspace()` table the inbox port used before the two
/// helpers were merged: `py_isspace` agrees with it on every character.
fn table(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0b}'
            | '\u{0c}'
            | '\r'
            | '\u{1c}'..='\u{1f}'
            | ' '
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

#[test]
fn isspace_is_pythons_on_every_character() {
    for c in (0..=0x10FFFF).filter_map(char::from_u32) {
        assert_eq!(py_isspace(c), table(c), "{c:?}");
    }
    assert!(!'\u{1c}'.is_whitespace(), "why py_isspace exists");
}

#[test]
fn line_breaks_and_word_characters() {
    for c in ['\n', '\r', '\u{1e}', '\u{85}', '\u{2029}'] {
        assert!(py_line_break(c), "{c:?}");
    }
    for c in ['\u{1f}', ' ', '\t', '\u{a0}'] {
        assert!(!py_line_break(c), "{c:?}");
    }
    for c in ['a', 'Z', '7', '_', 'é', '٣'] {
        assert!(py_word(c), "{c:?}");
    }
    for c in ['-', ' ', '.', '\u{200b}'] {
        assert!(!py_word(c), "{c:?}");
    }
}
