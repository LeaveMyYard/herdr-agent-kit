use super::*;

#[test]
fn db_path_precedence() {
    let home = Some(PathBuf::from("/home/u"));
    assert_eq!(
        db_path_from(Some("/x.db".into()), Some("/s".into()), home.clone()),
        PathBuf::from("/x.db")
    );
    assert_eq!(
        db_path_from(Some("".into()), Some("/s".into()), home.clone()),
        PathBuf::from("/s/herdr-inbox/inbox.db")
    );
    assert_eq!(
        db_path_from(None, Some("".into()), home),
        PathBuf::from("/home/u/.local/state/herdr-inbox/inbox.db")
    );
}
