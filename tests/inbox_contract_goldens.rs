//! The `inbox_contract` ports against the goldens in `fixtures/inbox/`, which were
//! captured from herdr-inbox's and herdr-panorama's Python and the herdr-inbox-ui mod
//! (`fixtures/inbox/capture/`). Every case in a golden file is checked.

use std::collections::BTreeSet;
use std::path::PathBuf;

use herdr_agent_kit::inbox_contract::delivery::{self, Answered};
use herdr_agent_kit::inbox_contract::open_args;
use herdr_agent_kit::inbox_contract::receipt;
use serde_json::Value;

fn golden(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/inbox")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| s(x).to_owned())
        .collect()
}

fn ids(v: &Value) -> BTreeSet<u64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .collect()
}

#[test]
fn delivery_encoding_matches_the_inbox() {
    let g = golden("delivery.json");
    assert_eq!(g["max_arg"].as_u64().unwrap() as usize, delivery::MAX_ARG);
    let cases = g["encode"].as_array().unwrap();
    assert!(cases.len() >= 30);
    for c in cases {
        let name = s(&c["name"]);
        let qs: Vec<Answered<'_>> = c["questions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| Answered {
                id: q["id"].as_u64().unwrap(),
                question: s(&q["question"]),
                answer: s(&q["answer"]),
            })
            .collect();
        assert_eq!(
            delivery::delivery_text(&qs),
            s(&c["delivery_text"]),
            "{name}"
        );
        let lines: Vec<String> = qs.iter().map(delivery::answer_line).collect();
        assert_eq!(lines, strs(&c["answer_lines"]), "{name}");
        let refs: Vec<String> = qs.iter().map(delivery::long_answer_reference).collect();
        assert_eq!(refs, strs(&c["references"]), "{name}");
        let msgs = delivery::delivery_messages(&qs, c["max_arg"].as_u64().unwrap() as usize);
        let want = c["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), want.len(), "{name}");
        for (m, w) in msgs.iter().zip(want) {
            assert_eq!(m.text, s(&w["text"]), "{name}");
            assert_eq!(
                m.ids,
                w["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap())
                    .collect::<Vec<_>>(),
                "{name}"
            );
            assert_eq!(
                m.acknowledged,
                w["acknowledged"].as_bool().unwrap(),
                "{name}"
            );
        }
    }
}

#[test]
fn first_line_and_answer_format_match_the_inbox() {
    let g = golden("delivery.json");
    for c in g["first_line"].as_array().unwrap() {
        let width = c["width"].as_u64().unwrap() as usize;
        assert_eq!(
            delivery::first_line(s(&c["text"]), width),
            s(&c["out"]),
            "{c}"
        );
    }
    for c in g["format_answer"].as_array().unwrap() {
        let choices = strs(&c["choices"]);
        let choices: Vec<&str> = choices.iter().map(String::as_str).collect();
        let notes: Vec<(&str, &str)> = c["notes"]
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.as_str(), s(v))).collect())
            .unwrap_or_default();
        let text = c["text"].as_str();
        assert_eq!(
            delivery::format_answer(&choices, &notes, text),
            s(&c["out"]),
            "{c}"
        );
    }
}

#[test]
fn delivery_parsing_matches_the_inbox_and_the_mod() {
    let g = golden("delivery.json");
    let cases = g["parse"].as_array().unwrap();
    assert!(
        cases
            .iter()
            .filter(|c| c["from_captured_ts"].as_bool().unwrap())
            .count()
            >= 5
    );
    for c in cases {
        let text = s(&c["text"]);
        assert_eq!(
            delivery::header_ids(text),
            ids(&c["header_ids"]),
            "header_ids {text:?}"
        );
        let rewake: Vec<&str> = delivery::rewake_texts(text);
        assert_eq!(rewake, strs(&c["rewake_texts"]), "rewake_texts {text:?}");
        for (got, key) in [
            (delivery::parse_answers(text), "answers"),
            (delivery::rewake_answers(text), "rewake_answers"),
        ] {
            let want: Vec<(String, String, String)> = c[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| {
                    (
                        s(&a["id"]).to_owned(),
                        s(&a["question"]).to_owned(),
                        s(&a["answer"]).to_owned(),
                    )
                })
                .collect();
            let got: Vec<(String, String, String)> = got
                .into_iter()
                .map(|a| (a.id, a.question, a.answer))
                .collect();
            assert_eq!(got, want, "{key} {text:?}");
        }
    }
}

#[test]
fn receipts_match_the_inbox_server_and_panorama() {
    let g = golden("receipts.json");
    let k = &g["constants"];
    assert_eq!(s(&k["unavailable"]["status"]), receipt::UNAVAILABLE);
    assert_eq!(s(&k["unavailable"]["message"]), receipt::NOT_SERVED_MESSAGE);
    assert_eq!(s(&k["not_migrated"]["status"]), receipt::UNAVAILABLE);
    assert_eq!(
        s(&k["not_migrated"]["message"]),
        receipt::NOT_MIGRATED_MESSAGE
    );
    let calls = g["calls"].as_array().unwrap();
    let mut statuses = BTreeSet::new();
    for c in calls {
        let name = s(&c["name"]);
        let text = s(&c["text"]);
        assert_eq!(
            receipt::question_ids(text),
            ids(&c["question_ids"]),
            "{name}"
        );
        // the text is the structured result as JSON
        let structured = &c["result"]["structuredContent"];
        assert_eq!(
            &serde_json::from_str::<Value>(text).unwrap(),
            structured,
            "{name}"
        );
        let entries: Vec<&Value> = match structured["questions"].as_array() {
            Some(qs) => {
                let pairs: Vec<(u64, &str)> = qs
                    .iter()
                    .map(|q| (q["id"].as_u64().unwrap(), s(&q["status"])))
                    .collect();
                assert_eq!(
                    receipt::filed_message(&pairs),
                    s(&structured["message"]),
                    "{name}"
                );
                qs.iter().collect()
            }
            None => vec![structured],
        };
        for e in entries {
            let status = s(&e["status"]);
            statuses.insert(status.to_owned());
            let want = s(&e["message"]);
            match status {
                receipt::OPEN => assert_eq!(receipt::open_message(e["id"].as_u64().unwrap()), want),
                receipt::ALREADY_OPEN => assert_eq!(
                    receipt::already_open_message(e["id"].as_u64().unwrap()),
                    want
                ),
                receipt::ANSWERED => {
                    assert_eq!(
                        receipt::answered_message(e["id"].as_u64().unwrap(), s(&e["answer"])),
                        want
                    )
                }
                receipt::UNAVAILABLE => assert_eq!(receipt::NOT_SERVED_MESSAGE, want),
                _ => {} // tool argument errors: the MCP server's, not this contract's
            }
        }
    }
    for c in g["question_ids_extra"].as_array().unwrap() {
        // Python's ints have no limit; an id that does not fit a u64 is dropped, never wrapped
        let want: BTreeSet<u64> = c["ids"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_u64)
            .collect();
        assert_eq!(receipt::question_ids(s(&c["text"])), want, "{c}");
    }
    for st in [
        receipt::OPEN,
        receipt::ALREADY_OPEN,
        receipt::ANSWERED,
        receipt::UNAVAILABLE,
        "error",
    ] {
        assert!(statuses.contains(st), "no golden for status {st}");
    }
}

#[test]
fn open_args_match_panorama() {
    let g = golden("open-args.json");
    for c in g["encode"].as_array().unwrap() {
        let question = c["question"].as_u64();
        let pane = c["pane"].as_str();
        let got = open_args::deep_link_args(question, pane, s(&c["version"]), c["help"].as_str());
        let calls = c["calls"].as_array().unwrap();
        assert_eq!(calls.len(), 1, "{c}");
        match calls[0].get("args") {
            Some(args) => assert_eq!(got, Some(strs(args)), "{c}"),
            None => {
                assert_eq!(got, None, "{c}");
                assert_eq!(s(&calls[0]["action"]), "leavemyyard.herdr-inbox.open");
            }
        }
    }
}

#[test]
fn open_args_parse_like_the_inbox() {
    let g = golden("open-args.json");
    for c in g["parse"].as_array().unwrap() {
        let argv = strs(&c["argv"]);
        let got = open_args::parse_open_args(&argv);
        if c["exit"].as_i64() == Some(2) {
            let err = got.expect_err(&format!("{argv:?} must be refused"));
            let want = s(&c["error"]);
            // the message matches argparse's, except that Python quotes the value with repr()
            let printable = argv
                .iter()
                .all(|a| a.chars().all(|c| c.is_ascii_graphic() || c == ' '));
            assert!(
                !printable || want.ends_with(&err.to_string()),
                "{argv:?}: {err} vs {want}"
            );
            continue;
        }
        let req = got.unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        match c.get("request") {
            None => assert_eq!(req.target, None, "{argv:?}"),
            Some(r) => {
                let t = req
                    .target
                    .as_ref()
                    .unwrap_or_else(|| panic!("{argv:?}: no target"));
                assert_eq!(t.kind.as_str(), s(&r["kind"]), "{argv:?}");
                assert_eq!(t.value, s(&r["value"]), "{argv:?}");
                assert_eq!(req.hold, r["hold"].as_bool().unwrap_or(true), "{argv:?}");
            }
        }
    }
}

#[cfg(feature = "sqlite")]
mod questions_db {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use herdr_agent_kit::inbox_contract::sqlite::{OpenQuestion, open_questions};
    use rusqlite::Connection;
    use serde_json::Value;

    use super::{golden, s};

    fn tmp(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("inbox-contract");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        for ext in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        path
    }

    /// A database rebuilt from the dump of one the inbox's own code made.
    fn from_dump(dump: &str, name: &str) -> PathBuf {
        let path = tmp(name);
        let sql = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/inbox/questions-db")
                .join(format!("{dump}.sql")),
        )
        .unwrap();
        let con = Connection::open(&path).unwrap();
        // the bundled SQLite enforces foreign keys by default, Python's does not, and the
        // dump creates tables in name order (question_ext before questions)
        con.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        con.execute_batch(&sql).unwrap();
        path
    }

    fn expected(name: &str) -> Option<Vec<OpenQuestion>> {
        let g = golden("questions-db/expected.json");
        let read = g["reads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["db"] == name)
            .unwrap()
            .clone();
        let opt = |v: &Value| v.as_str().map(str::to_owned);
        read["questions"].as_array().map(|qs| {
            qs.iter()
                .map(|q| OpenQuestion {
                    id: q["id"].as_i64().unwrap(),
                    session_id: opt(&q["session_id"]),
                    pane_id: opt(&q["pane_id"]),
                    asked_by: opt(&q["asked_by"]),
                    question: s(&q["question"]).to_owned(),
                    created_at: opt(&q["created_at"]),
                })
                .collect()
        })
    }

    fn reads_like_panorama(name: &str, path: &Path) {
        assert_eq!(open_questions(path), expected(name), "{name}");
    }

    #[test]
    fn current_and_0_1_databases() {
        for name in ["current", "v01"] {
            let path = from_dump(name, &format!("{name}.db"));
            let before = std::fs::read(&path).unwrap();
            reads_like_panorama(name, &path);
            assert_eq!(std::fs::read(&path).unwrap(), before, "read-only");
        }
    }

    #[test]
    fn no_inbox() {
        reads_like_panorama("no-table", &from_dump("no-table", "no-table.db"));
        reads_like_panorama("missing", &tmp("missing.db"));
        let dir = tmp("a-directory.db");
        std::fs::create_dir_all(&dir).unwrap();
        reads_like_panorama("directory", &dir);
        let junk = tmp("junk.db");
        std::fs::write(
            &junk,
            b"not a database at all, just text that is long enough",
        )
        .unwrap();
        assert_eq!(open_questions(&junk), None);
    }

    /// A writer holding the write lock: a WAL database (the inbox's own mode) still reads;
    /// a rollback-journal one reads as no inbox once the busy timeout passes.
    #[test]
    fn a_held_write_lock() {
        for (name, mode) in [
            ("v01-wal-writer-busy", "WAL"),
            ("v01-rollback-locked", "DELETE"),
        ] {
            let path = from_dump("v01", &format!("{name}.db"));
            let (locked, release) = (mpsc::channel(), mpsc::channel::<()>());
            let (locked_tx, locked_rx) = locked;
            let (release_tx, release_rx) = release;
            let holder_path = path.clone();
            let holder = thread::spawn(move || {
                let con = Connection::open(&holder_path).unwrap();
                let got: String = con
                    .query_row(&format!("PRAGMA journal_mode={mode}"), [], |r| r.get(0))
                    .unwrap();
                assert_eq!(got.to_uppercase(), mode);
                con.execute_batch(
                    "BEGIN EXCLUSIVE; UPDATE questions SET repeat_count = repeat_count + 1;",
                )
                .unwrap();
                locked_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                con.execute_batch("ROLLBACK").unwrap();
            });
            locked_rx.recv().unwrap();
            let started = Instant::now();
            reads_like_panorama(name, &path);
            if mode == "DELETE" {
                let waited = started.elapsed();
                assert!(
                    waited >= Duration::from_millis(900),
                    "waited for the lock first"
                );
                // rusqlite's own default is 5 s: this proves BUSY_TIMEOUT (1 s) is set
                assert!(
                    waited < Duration::from_secs(3),
                    "gave up after BUSY_TIMEOUT: {waited:?}"
                );
            }
            release_tx.send(()).unwrap();
            holder.join().unwrap();
        }
    }
}
