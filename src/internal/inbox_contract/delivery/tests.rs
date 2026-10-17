use std::collections::BTreeSet;

use super::*;

fn a<'a>(id: u64, question: &'a str, answer: &'a str) -> Answered<'a> {
    Answered {
        id,
        question,
        answer,
    }
}

#[test]
fn single_and_batch_round_trip() {
    let one = delivery_text(&[a(12, "Which DB?", "Postgres")]);
    assert_eq!(one, "[inbox] Q12 answered — Which DB?: Postgres");
    assert_eq!(header_ids(&one), BTreeSet::from([12]));
    let two = delivery_text(&[a(1, "a?", "b"), a(2, "c?", "d")]);
    assert_eq!(two, "[inbox] answered — Q1 a?: b · Q2 c?: d");
    assert_eq!(header_ids(&two), BTreeSet::from([1, 2]));
    let parsed = parse_answers(&two);
    assert_eq!(
        parsed.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["Q1", "Q2"]
    );
    assert_eq!(
        (parsed[1].question.as_str(), parsed[1].answer.as_str()),
        ("c?", "d")
    );
}

#[test]
fn a_quoted_header_is_not_a_delivery() {
    let quoted = "why did you get \"[inbox] Q12 answered — Which DB?: Postgres\" twice?";
    assert!(header_ids(quoted).is_empty());
    assert!(parse_answers(quoted).is_empty());
}

#[test]
fn first_line_counts_code_points() {
    assert_eq!(first_line(&"\u{1F680}".repeat(5), 3), "\u{1F680}\u{1F680}…");
    assert_eq!(first_line("hello", 0), "hell…");
    assert_eq!(first_line(" \u{1c}a\u{1d}b", 80), "a");
}

#[test]
fn an_over_long_answer_goes_alone_as_a_reference() {
    let long = "x".repeat(MAX_ARG);
    let qs = [a(1, "a?", "b"), a(2, "Long?", &long), a(3, "c?", "d")];
    let msgs = delivery_messages(&qs, MAX_ARG);
    assert_eq!(
        msgs.iter()
            .map(|m| (m.ids.clone(), m.acknowledged))
            .collect::<Vec<_>>(),
        [(vec![2], false), (vec![1, 3], true)]
    );
    assert!(msgs[0].text.contains("(long answer, 4000 chars;"));
}
