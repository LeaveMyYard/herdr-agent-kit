use super::*;

#[test]
fn panorama_args_round_trip_through_the_inbox_parser() {
    let args = deep_link_args(Some(7), None, "0.10.6", Some("[--no-hold]")).unwrap();
    assert_eq!(args, ["open", "--question", "Q7", "--no-hold"]);
    let req = parse_open_args(&args[1..]).unwrap();
    assert_eq!(
        req,
        OpenRequest {
            target: Some(OpenTarget {
                kind: OpenKind::Question,
                value: "7".into()
            }),
            hold: false
        }
    );
}

#[test]
fn question_ids_differ_from_python_int_only_out_of_range() {
    // Python's int() reads both (-1, a 23-digit int); no question has such an id.
    // Everything else question_id reads is pinned by the open-args.json goldens.
    for (v, want) in [("Q-1", None), ("Q99999999999999999999999", None)] {
        assert_eq!(question_id(v), want, "{v:?}");
    }
}
