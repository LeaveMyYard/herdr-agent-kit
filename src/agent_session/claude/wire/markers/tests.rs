use super::*;

#[test]
fn wanted_lines_matches_wanted() {
    let lines: [&[u8]; 5] = [
        br#"{"type":"assistant","x":"\"name\":\"Bash\""}"#,
        br#"{"name":"Agent"}"#,
        br#"{"role":"user","content":"hi"}"#,
        br#"{"role":"user","content":[]}"#,
        b"<task-notification>",
    ];
    let by_one: Vec<usize> = (0..lines.len()).filter(|&i| wanted(lines[i])).collect();
    assert_eq!(wanted_lines(&lines), by_one);
    assert_eq!(by_one, vec![1, 2, 4]);
}
