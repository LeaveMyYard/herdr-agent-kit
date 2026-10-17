use super::*;

/// panorama tests/test_herdr.py::test_clean_strips_controls_and_caps
#[test]
fn clean_strips_controls_and_caps() {
    assert_eq!(clean("a\x1b[31mb\tc\nd\x07", MAX_ARG), "a[31mb c d");
    assert_eq!(clean(&"x".repeat(5000), MAX_ARG).chars().count(), 4000);
}

/// Every input in `fixtures/herdr/clean/clean.jsonl`, captured from both
/// Python plugins, gives the same output here.
#[test]
fn clean_matches_the_captured_python_outputs() {
    let goldens = include_str!("../../../fixtures/herdr/clean/clean.jsonl");
    let mut n = 0;
    for line in goldens.lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let text = row["text"].as_str().unwrap();
        let limit = row["limit"].as_u64().map_or(MAX_ARG, |l| l as usize);
        assert_eq!(
            clean(text, limit),
            row["out"].as_str().unwrap(),
            "input {text:?} limit {limit}"
        );
        n += 1;
    }
    assert!(n > 200, "{n} goldens");
}

#[test]
fn clean_is_idempotent_on_its_own_output() {
    for text in ["a\x1b[1mb", &"y".repeat(4001), "a\u{2028}b"] {
        let once = clean(text, MAX_ARG);
        assert_eq!(clean(&once, MAX_ARG), once);
    }
}

#[test]
fn sanitized_is_built_only_through_clean() {
    assert_eq!(Sanitized::new("a\nb\x1b").as_str(), "a b");
    assert_eq!(Sanitized::with_limit("abcdef", 3).as_str(), "ab…");
    assert_eq!(Sanitized::from("x\ty".to_string()).to_string(), "x y");
}
