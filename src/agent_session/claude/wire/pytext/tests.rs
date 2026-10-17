use super::*;
use serde_json::json;

#[test]
fn splitlines_matches_python() {
    assert_eq!(
        py_splitlines("a\r\nb\rc\nd\x0be\u{2028}f"),
        vec!["a", "b", "c", "d", "e", "f"]
    );
    assert_eq!(py_splitlines("a\n"), vec!["a"]);
    assert_eq!(py_splitlines(""), Vec::<&str>::new());
    assert_eq!(py_splitlines("\n"), vec![""]);
}

#[test]
fn python_str_of_values() {
    assert_eq!(py_str(&json!("s")), "s");
    assert_eq!(py_str(&json!(5)), "5");
    assert_eq!(py_str(&json!(1.5)), "1.5");
    assert_eq!(py_str(&json!(true)), "True");
    assert_eq!(py_str(&Value::Null), "None");
    for (f, want) in [
        (1791969600.0, "1791969600.0"),
        (0.1, "0.1"),
        (1e16, "1e+16"),
        (0.00001, "1e-05"),
        (-2.5e-7, "-2.5e-07"),
        (123456789012345.6, "123456789012345.6"),
    ] {
        assert_eq!(py_float_repr(f), want);
    }
}
