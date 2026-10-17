use super::*;

#[test]
fn basename_is_posix() {
    assert_eq!(basename("/a/b/claude"), "claude");
    assert_eq!(basename("claude"), "claude");
    assert_eq!(basename("/a/claude/"), "");
    assert_eq!(basename(""), "");
}
