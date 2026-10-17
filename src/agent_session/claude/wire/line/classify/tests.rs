//! The title and attachment branches of the record match. Expected values
//! captured 2026-10-18 from the Python plugins, not written by hand:
//! titles from herdr-panorama 83590dc (`scan.session.Session.update` over a
//! one-record transcript, `Session.titles`); attachment texts from
//! herdr-inbox 3949b21 (`transcript._record_texts`).

use serde_json::{Value, json};

use super::*;

fn line(v: &Value) -> Line {
    Line::parse(v.to_string().as_bytes()).unwrap()
}

#[test]
fn titles_by_kind_as_panorama_reads_them() {
    // panorama: {"custom": "c"}, {"agent": "a"}, {"ai": "i"}, then no title
    let cases = [
        (
            json!({"type": "custom-title", "customTitle": "c"}),
            Some((TitleKind::Custom, "c")),
        ),
        (
            json!({"type": "agent-name", "agentName": "a"}),
            Some((TitleKind::Agent, "a")),
        ),
        (
            json!({"type": "ai-title", "aiTitle": "i"}),
            Some((TitleKind::Ai, "i")),
        ),
        (json!({"type": "custom-title"}), None),
        (json!({"type": "agent-name", "agentName": 5}), None),
        (
            json!({"type": "ai-title", "customTitle": "wrong key"}),
            None,
        ),
    ];
    for (rec, want) in cases {
        let want = match want {
            Some((kind, title)) => Record::Title(kind, title),
            None => Record::Other,
        };
        assert_eq!(line(&rec).record(), want, "{rec}");
    }
}

#[test]
fn attachment_texts_as_the_inbox_reads_them() {
    // inbox `_record_texts`: ["x", "y"], ["s"], [], [], then ["p"], []
    let hook = |content: Option<Value>| {
        let mut att = json!({"type": "hook_additional_context"});
        if let Some(c) = content {
            att["content"] = c;
        }
        json!({"type": "attachment", "attachment": att})
    };
    let cases: [(Value, Attachment); 6] = [
        (
            hook(Some(json!(["x", 3, "y"]))),
            Attachment::HookAdditionalContext {
                content: vec!["x", "y"],
            },
        ),
        (
            hook(Some(json!("s"))),
            Attachment::HookAdditionalContext { content: vec!["s"] },
        ),
        (
            hook(Some(json!(7))),
            Attachment::HookAdditionalContext { content: vec![] },
        ),
        (
            hook(None),
            Attachment::HookAdditionalContext { content: vec![] },
        ),
        (
            json!({"type": "attachment", "attachment": {"type": "queued_command", "prompt": "p"}}),
            Attachment::QueuedCommand { prompt: Some("p") },
        ),
        (
            json!({"type": "attachment", "attachment": {"type": "queued_command", "prompt": 1}}),
            Attachment::QueuedCommand { prompt: None },
        ),
    ];
    for (rec, want) in cases {
        assert_eq!(line(&rec).record(), Record::Attachment(want), "{rec}");
    }
}
