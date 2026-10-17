use super::*;

#[test]
fn ids_stand_as_words() {
    assert_eq!(
        question_ids("Recorded as Q5. names Q5."),
        BTreeSet::from([5])
    );
    assert!(question_ids("xQ5 Q5x _Q5 Q").is_empty());
    assert_eq!(question_ids("(Q7) Q8,Q9"), BTreeSet::from([7, 8, 9]));
}

#[test]
fn json_entries_count_only_when_open() {
    assert_eq!(
        question_ids(r#"{"id": 3, "status": "open"}"#),
        BTreeSet::from([3])
    );
    assert_eq!(
        question_ids(r#"{"id":4 ,"status" :"already_open"}"#),
        BTreeSet::from([4])
    );
    assert!(question_ids(r#"{"id": 5, "status": "answered"}"#).is_empty());
    assert!(question_ids(r#"{"id": 6, "status": "opened"}"#).is_empty());
}

#[test]
fn question_ids_and_result_text() {
    let ids = |t: &str| question_ids(t).into_iter().collect::<Vec<_>>();
    assert_eq!(ids("Recorded as Q12. Continue."), vec![12]);
    assert_eq!(ids("Q3 and Q45, notQ7, Q8x"), vec![3, 45]);
    assert_eq!(
        ids(r#"{"id": 9, "status": "already_open"} {"id":10,"status":"answered"}"#),
        vec![9]
    );
}

#[test]
fn ask_operator_calls_are_named_by_their_suffix() {
    assert!(is_ask_operator(
        "mcp__plugin_herdr-inbox_herdr-inbox__ask_operator"
    ));
    assert!(is_ask_operator("mcp__inbox__ask_operator"));
    assert!(!is_ask_operator("mcp__inbox__check_answers"));
    assert!(!is_ask_operator("ask_operator_later"));
}
