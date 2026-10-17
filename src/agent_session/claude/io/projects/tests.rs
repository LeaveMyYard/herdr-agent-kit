use super::*;

#[test]
fn projects_root_resolution() {
    let home = Some(PathBuf::from("/home/u"));
    assert_eq!(
        ProjectsRoot::resolve(None, home.clone()).unwrap().path(),
        Path::new("/home/u/.claude/projects")
    );
    assert_eq!(
        ProjectsRoot::resolve(Some("".into()), home.clone())
            .unwrap()
            .path(),
        Path::new("/home/u/.claude/projects")
    );
    assert_eq!(
        ProjectsRoot::resolve(Some("/cfg".into()), home)
            .unwrap()
            .path(),
        Path::new("/cfg/projects")
    );
    assert_eq!(
        ProjectsRoot::resolve(Some("/cfg".into()), None)
            .unwrap()
            .path(),
        Path::new("/cfg/projects")
    );
    assert_eq!(ProjectsRoot::resolve(None, None), None);
}

#[test]
fn sanitized_project_names() {
    assert_eq!(sanitize_cwd("/home/user/project"), "-home-user-project");
    assert_eq!(sanitize_cwd("/a b/é_x.y"), "-a-b---x-y");
}
