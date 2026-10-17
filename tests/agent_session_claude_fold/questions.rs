//! `ask_operator` results record the asker's question ids.

use super::*;

#[test]
fn ask_operator_results_record_the_askers_question_ids() {
    let mut sc = Sc::new("qids");
    bg_agent(&mut sc, "a1", "x");
    let tx = &mut sc.agents.get_mut("a1").unwrap().0;
    let id = tx.tid();
    let name = "mcp__plugin_herdr-inbox_herdr-inbox__ask_operator";
    tx.add(
        tool_block(
            tpl("assistant_ask_operator"),
            name,
            &id,
            json!({"questions": [{"question": "q"}]}),
        ),
        B + M,
    );
    let text = json!({"group": 7, "questions": [{"id": 7, "status": "open", "message": "Recorded as Q7. Continue."}]}).to_string();
    tx.add(
        result_block(tpl("result_ask_operator"), &id, &text, None),
        B + M + 1.0,
    );
    let s = sc.load();
    let ids: Vec<u64> = s
        .session()
        .qids(&Owner::Agent("a1".into()))
        .into_iter()
        .flatten()
        .copied()
        .collect();
    assert_eq!(ids, vec![7]);
}
