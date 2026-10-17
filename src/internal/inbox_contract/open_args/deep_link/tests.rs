use super::*;

#[test]
fn versions() {
    assert_eq!(version_prefix("0.10.6"), (0, 10, 6));
    assert_eq!(version_prefix("0.6"), (0, 6, 0));
    assert_eq!(version_prefix("1.0.0-rc1"), (1, 0, 0));
    assert_eq!(version_prefix("v1.0.0"), (0, 0, 0));
    assert_eq!(version_prefix("7"), (0, 0, 0));
    assert_eq!(
        version_prefix("99999999999999999999999.1"),
        (u64::MAX, 1, 0)
    );
}
