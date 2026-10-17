//! `toml10::parse` against what Python's `tomllib` made of the same
//! inputs, captured by `fixtures/toml10/capture/capture_tomllib.py`.

use super::parse;
use serde_json::{Value as Json, json};
use toml::Value;
use toml::value::{Datetime, Offset};

const CASES: &str = include_str!("../../fixtures/toml10/cases.json");

/// Inputs `tomllib` accepts and the `toml` crate cannot: integers outside
/// i64, a float literal beyond f64 (`tomllib` makes it infinite) and nesting
/// deeper than the crate's recursion limit. Rust ignores such a file whole;
/// the Python plugins apply it.
const RUST_REFUSES: &[&str] = &[
    "int-over-max",
    "int-under-min",
    "hex-over-max",
    "float-overflow",
    "deep-array",
    "deep-inline",
];

fn tag(value: &Value) -> Json {
    let scalar = |t: &str, v: String| json!({ "type": t, "value": v });
    match value {
        Value::Table(t) => Json::Object(t.iter().map(|(k, v)| (k.clone(), tag(v))).collect()),
        Value::Array(a) => Json::Array(a.iter().map(tag).collect()),
        Value::String(s) => scalar("string", s.clone()),
        Value::Integer(i) => scalar("integer", i.to_string()),
        Value::Float(f) if f.is_nan() => scalar("float", "nan".into()),
        Value::Float(f) => scalar("float", format!("{:016x}", f.to_bits())),
        Value::Boolean(b) => scalar("bool", b.to_string()),
        Value::Datetime(d) => tag_datetime(d),
    }
}

/// The capture's ISO 8601 form: six fraction digits, offsets as `+HH:MM`.
fn tag_datetime(d: &Datetime) -> Json {
    let date = d
        .date
        .map(|d| format!("{:04}-{:02}-{:02}", d.year, d.month, d.day));
    let time = d.time.map(|t| {
        format!(
            "{:02}:{:02}:{:02}.{:06}",
            t.hour,
            t.minute,
            t.second,
            t.nanosecond / 1000
        )
    });
    let offset = d.offset.map(|o| {
        let minutes = match o {
            Offset::Z => 0,
            Offset::Custom { minutes } => minutes,
        };
        let sign = if minutes < 0 { '-' } else { '+' };
        let m = minutes.unsigned_abs();
        format!("{sign}{:02}:{:02}", m / 60, m % 60)
    });
    let (t, v) = match (date, time, offset) {
        (Some(d), Some(t), Some(o)) => ("datetime", format!("{d}T{t}{o}")),
        (Some(d), Some(t), None) => ("datetime-local", format!("{d}T{t}")),
        (Some(d), None, None) => ("date-local", d),
        (None, Some(t), None) => ("time-local", t),
        other => panic!("unexpected datetime shape {other:?}"),
    };
    json!({ "type": t, "value": v })
}

fn input(case: &Json) -> Result<String, ()> {
    if let Some(text) = case["toml"].as_str() {
        return Ok(text.to_owned());
    }
    let hex = case["toml_hex"].as_str().expect("toml or toml_hex");
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    // ConfigFile reads with read_to_string: not UTF-8 is an ignored file.
    String::from_utf8(bytes).map_err(|_| ())
}

#[test]
fn parses_as_tomllib_does() {
    let goldens: Json = serde_json::from_str(CASES).unwrap();
    let cases = goldens["cases"].as_array().unwrap();
    assert!(cases.len() > 800, "the corpus is missing");
    let mut failures = Vec::new();
    let mut known = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let python = &case["python"];
        let rust = input(case)
            .map_err(|()| "not UTF-8".to_owned())
            .and_then(|t| parse(&t));
        match (python.get("ok"), &rust) {
            (Some(expected), Ok(table)) => {
                let got = tag(&Value::Table(table.clone()));
                if &got != expected {
                    failures.push(format!("{name}: tomllib {expected}, here {got}"));
                }
            }
            (Some(_), Err(e)) if RUST_REFUSES.contains(&name) => known.push(format!("{name}: {e}")),
            (Some(_), Err(e)) => failures.push(format!("{name}: tomllib accepts, here: {e}")),
            // A tomllib error (or exception) is an ignored file in both.
            (None, Err(_)) => {}
            (None, Ok(_)) => failures.push(format!("{name}: tomllib {python}, accepted here")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(known.len(), RUST_REFUSES.len(), "{known:#?}");
}
