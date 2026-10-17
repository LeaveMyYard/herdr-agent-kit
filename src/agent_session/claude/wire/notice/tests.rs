use super::*;

/// Expected values from panorama's `wire.parse_notices` / `question_ids`
/// / `text_of` on main (`83590dc`), run over the same inputs.
#[test]
fn notices_parse_as_panorama_reads_them() {
    let n =
        |task: &str, tool_use: Option<&str>, status: Option<Ending>, uses: Option<u64>| Notice {
            task: task.into(),
            tool_use: tool_use.map(str::to_string),
            status,
            tool_uses: uses,
        };
    let cases: [(&str, Vec<Notice>); 7] = [
        (
            "<task-notification>\n<task-id>a1</task-id>\n<tool-use-id>toolu_1</tool-use-id>\n<status>completed</status>\n<usage><tool_uses>12</tool_uses></usage>\n</task-notification>",
            vec![n("a1", Some("toolu_1"), Some(Ending::Done), Some(12))],
        ),
        (
            // the notice a resumed session gets for its orphaned tasks: every id, not the marker
            "  <task-notification><task-id>b1</task-id>\n<task-id>b2</task-id>\n<task-id>__orphan_summary__:shell</task-id>\n<status>stopped</status></task-notification>",
            vec![
                n("b1", None, Some(Ending::Cancelled), None),
                n("b2", None, Some(Ending::Cancelled), None),
            ],
        ),
        (
            "<task-notification><task-id>m1</task-id><event>[Monitor expired after 30m]</event></task-notification>",
            vec![n("m1", None, Some(Ending::Done), None)],
        ),
        (
            "<task-notification><task-id>m1</task-id><event>line 4</event></task-notification>",
            vec![n("m1", None, None, None)],
        ),
        (
            // a repeated tag counts with its last value; two blocks in one carrier
            "<task-notification><task-id> x1 </task-id><status>failed</status><status>killed</status><tool_uses>n/a</tool_uses></task-notification><task-notification><task-id>x2</task-id><status>error</status></task-notification>",
            vec![
                n("x1", None, Some(Ending::Cancelled), None),
                n("x2", None, Some(Ending::Failed), None),
            ],
        ),
        (
            "text <task-notification><task-id>a1</task-id><status>completed</status></task-notification>",
            vec![],
        ),
        (
            "<task-notification><task-id>__orphan_summary__</task-id><status>stopped</status></task-notification>",
            vec![],
        ),
    ];
    for (text, want) in cases {
        assert_eq!(parse_notices(text), want, "{text}");
    }
}
