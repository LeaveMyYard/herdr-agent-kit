//! herdr-panorama's model goldens (plan §5.2 item 3), reproduced byte for
//! byte by the kit's `claude_session` fold plus a test-only port of
//! panorama's model builder (`panorama_model`).
//!
//! Each golden (`fixtures/panorama/model/*.json`) is `panorama snapshot
//! --json` over one `tests/scenario.py` scenario (`fixtures/panorama/inputs/`)
//! at fixed `now` values, captured from the bridged Python by panorama's
//! `tools/capture_goldens.py` (see `fixtures/panorama/README.md`). This test
//! materialises the scenario as that tool does (the files with their mtimes,
//! the events dir, the herdr snapshot, the inbox rows), runs one collect per
//! snapshot key, and writes the whole golden file again as the tool writes it
//! (`json.dumps(doc, indent=1, ensure_ascii=False)`): the bytes must match.

mod common;
mod panorama_model;

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use panorama_model::pyjson::{self, J};
use panorama_model::{Collector, Question};
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/panorama");

/// `tools/capture_goldens.py`'s `MODEL_TIMES`: minutes after the scenario's
/// own `now` (live, later, past the done window, stale).
const MODEL_TIMES: [(&str, &[i64]); 3] = [
    ("demo", &[0, 30, 180, 420, 780]),
    ("continued", &[0, 120]),
    ("background", &[0, 60]),
];
/// Its `CONDITIONS`, run on the demo at its own `now`, in this order.
const CONDITIONS: [&str; 2] = ["herdr down", "no inbox"];

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `materialize`: the scenario's files (each with its mtime) under `root`,
/// and its events dir.
fn materialize(input: &Value, root: &Path) {
    for (rel, f) in input["files"].as_object().expect("files") {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, f["text"].as_str().expect("text")).expect("write");
        let mtime = UNIX_EPOCH + Duration::from_secs_f64(f["mtime"].as_f64().expect("mtime"));
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|fh| fh.set_modified(mtime))
            .expect("set the mtime");
    }
    std::fs::create_dir_all(root.join(input["events"].as_str().expect("events"))).expect("mkdir");
}

/// The inbox rows `inbox.open_questions` returns: `status = 'open'`, by id.
fn open_questions(input: &Value) -> Vec<Question> {
    let mut rows: Vec<&Value> = input["inbox"]
        .as_array()
        .expect("inbox rows")
        .iter()
        .filter(|r| r["status"] == "open")
        .collect();
    rows.sort_by_key(|r| r["id"].as_i64());
    rows.into_iter()
        .map(|r| Question {
            id: r["id"].as_i64().expect("an id"),
            session_id: r["session"].as_str().map(str::to_string),
            pane_id: r["pane"].as_str().map(str::to_string),
            question: r["question"].as_str().unwrap_or("").to_string(),
        })
        .collect()
}

/// One `panorama snapshot --json` at `now`, with a condition's env applied:
/// `FAKE_HERDR_FAIL` naming `api snapshot` makes herdr fail; an
/// `HERDR_INBOX_DB` other than the scenario's database is a missing inbox.
fn snapshot(input: &Value, root: &Path, now: f64, env: Option<&Value>) -> J {
    let mut herdr_ok = true;
    let mut inbox_ok = true;
    for (key, value) in env.and_then(Value::as_object).into_iter().flatten() {
        let value = value.as_str().expect("an env value");
        match key.as_str() {
            "FAKE_HERDR_FAIL" => herdr_ok = !value.split(',').any(|c| c == "api snapshot"),
            "HERDR_INBOX_DB" => inbox_ok = value == "<root>/inbox.db",
            other => panic!("a condition this test does not model: {other}"),
        }
    }
    let questions = open_questions(input);
    let mut collector = Collector::new(
        &root.join(input["projects"].as_str().expect("projects")),
        &root.join(input["events"].as_str().expect("events")),
        &root.join("hooks"), // PANORAMA_HOOKS_DIR: no hook records in these scenarios
        now,
    );
    let swarm = collector.collect(
        herdr_ok.then_some(&input["snapshot"]),
        inbox_ok.then_some(questions.as_slice()),
    );
    // the capture replaced the materialised root with "<root>" in the printed JSON
    swarm.replace_in_strings(&root.to_string_lossy(), "<root>")
}

fn first_difference(want: &Value, got: &Value, at: &str) -> Option<String> {
    match (want, got) {
        (Value::Object(w), Value::Object(g)) => {
            for (k, wv) in w {
                let Some(gv) = g.get(k) else {
                    return Some(format!("{at}.{k}: missing"));
                };
                if let Some(d) = first_difference(wv, gv, &format!("{at}.{k}")) {
                    return Some(d);
                }
            }
            g.keys()
                .find(|k| !w.contains_key(*k))
                .map(|k| format!("{at}.{k}: unexpected"))
        }
        (Value::Array(w), Value::Array(g)) => {
            if w.len() != g.len() {
                return Some(format!("{at}: {} items, want {}", g.len(), w.len()));
            }
            w.iter()
                .zip(g)
                .enumerate()
                .find_map(|(i, (wv, gv))| first_difference(wv, gv, &format!("{at}[{i}]")))
        }
        (w, g) if w == g => None,
        (w, g) => Some(format!("{at}: got {g}, want {w}")),
    }
}

fn check(name: &str, minutes: &[i64]) {
    let golden_path = Path::new(FIXTURES)
        .join("model")
        .join(format!("{name}.json"));
    let golden_text = std::fs::read_to_string(&golden_path).expect("the golden");
    let golden = read_json(&golden_path);
    let input = read_json(&Path::new(FIXTURES).join(golden["input"].as_str().expect("input")));
    let now = input["now"].as_f64().expect("now");
    assert_eq!(
        golden["now"].as_f64(),
        Some(now),
        "{name}: the golden's now"
    );
    let root = common::tmp_dir(&format!("model-{name}"));
    materialize(&input, &root);

    let mut runs: Vec<(String, f64, Option<&Value>)> = minutes
        .iter()
        .map(|m| (format!("+{m}m"), now + *m as f64 * 60.0, None))
        .collect();
    if name == "demo" {
        for cond in CONDITIONS {
            let key = format!("+0m {cond}");
            let env = golden["conditions"].get(&key).expect("the condition's env");
            runs.push((key, now, Some(env)));
        }
    }
    let mut snaps = Vec::new();
    for (key, at, env) in &runs {
        let swarm = snapshot(&input, &root, *at, *env);
        let got: Value = serde_json::from_str(&pyjson::dumps(&swarm, 2)).expect("valid JSON");
        let want = &golden["snapshots"][key];
        if let Some(d) = first_difference(want, &got, &format!("{name} {key}")) {
            panic!("model differs from the golden: {d}");
        }
        snaps.push((key.clone(), swarm));
    }
    // the golden file itself, as capture_goldens.write_json writes it
    let conditions = J::Obj(
        runs.iter()
            .filter_map(|(k, _, env)| env.map(|e| (k.clone(), J::from(e))))
            .collect(),
    );
    let doc = J::obj([
        ("source", J::from(&golden["source"])),
        ("input", J::from(&golden["input"])),
        ("now", J::Float(now)),
        ("conditions", conditions),
        ("snapshots", J::Obj(snaps)),
    ]);
    let written = pyjson::dumps(&doc, 1) + "\n";
    if written != golden_text {
        let line = written
            .lines()
            .zip(golden_text.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        panic!(
            "{name}: the golden file differs at line {}:\n got: {:?}\nwant: {:?}",
            line + 1,
            written.lines().nth(line),
            golden_text.lines().nth(line)
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_demo_model_matches_its_golden() {
    check("demo", MODEL_TIMES[0].1);
}

#[test]
fn the_continued_model_matches_its_golden() {
    check("continued", MODEL_TIMES[1].1);
}

#[test]
fn the_background_model_matches_its_golden() {
    check("background", MODEL_TIMES[2].1);
}

#[test]
fn every_golden_is_checked() {
    let mut names: Vec<String> = std::fs::read_dir(Path::new(FIXTURES).join("model"))
        .expect("the model goldens")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .into_string()
                .expect("utf-8")
        })
        .collect();
    names.sort();
    let mut known: Vec<String> = MODEL_TIMES
        .iter()
        .map(|(n, _)| format!("{n}.json"))
        .collect();
    known.sort();
    assert_eq!(names, known);
}
