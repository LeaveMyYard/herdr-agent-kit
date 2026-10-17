use super::*;

#[test]
fn negative_looking_values() {
    assert!(looks_negative("-1") && looks_negative("-.5") && looks_negative("-1.5"));
    assert!(!looks_negative("--no-hold") && !looks_negative("-") && !looks_negative("-1."));
    assert_eq!(
        parse_open_args(&["--question", "--no-hold"]),
        Err(OpenArgsError::MissingValue(OpenKind::Question))
    );
}
