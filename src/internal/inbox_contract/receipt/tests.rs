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
