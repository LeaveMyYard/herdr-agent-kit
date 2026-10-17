use super::*;
use serde_json::json;

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

#[test]
fn python_truthiness() {
    for v in [
        json!(null),
        json!(false),
        json!(0),
        json!(0.0),
        json!(""),
        json!([]),
        json!({}),
    ] {
        assert!(!truthy(&v), "{v}");
    }
    for v in [
        json!(true),
        json!(-1),
        json!(0.5),
        json!(" "),
        json!([0]),
        json!({"a": null}),
    ] {
        assert!(truthy(&v), "{v}");
    }
}
