//! The views' contract: what a getter returns borrows the session, not the
//! temporary view or the getter's arguments, so a function holding only
//! `&Session` can return it. These helpers must compile.

use super::*;
use herdr_agent_kit::agent_session::claude::Session;
use herdr_agent_kit::agent_session::claude::fold::{BgLaunch, Prompt, TaskEnd, WorkflowLaunch};

fn prompts(s: &Session) -> impl Iterator<Item = Prompt<'_>> {
    s.conversation().prompts()
}

fn notices<'s>(s: &'s Session, task: &str) -> impl Iterator<Item = TaskEnd> + 's {
    let task = task.to_string(); // the argument dies here: the iterator must not hold it
    s.evidence().notices(&task)
}

fn resumes_of_all(s: &Session, ids: &[&str]) -> Vec<Vec<Ts>> {
    ids.iter()
        .map(|i| s.evidence().resumes(i))
        .map(Iterator::collect)
        .collect()
}

fn children<'s>(s: &'s Session, parent: &Owner) -> impl Iterator<Item = &'s str> {
    s.parentage().children(parent)
}

fn launches(s: &Session) -> impl Iterator<Item = &WorkflowLaunch> {
    s.workflows().launches()
}

fn phases<'s>(s: &'s Session, run: &str) -> impl Iterator<Item = &'s str> {
    s.workflows().phases(run)
}

fn waiting<'s>(s: &'s Session, owner: &Owner, now: Ts) -> impl Iterator<Item = &'s BgLaunch> {
    s.background().waiting(owner, now, &NoProcs)
}

#[test]
fn view_getters_borrow_the_session_not_the_view() {
    let mut sc = Sc::new("views");
    sc.main.prompt(B - 60.0, "go");
    let call = bg_agent(&mut sc, "p1", "parent");
    sc.main
        .notify(B + 2.0 * M, "p1", &call, "completed", &["queue"]);
    sc.main.resume(B + 3.0 * M, "p1");
    sc.tx("p1").launch(B + M, "c1", "child", true);
    sc.tx("p1").bash_background(B + M, "b1");
    let scanner = sc.load();
    let s = scanner.session();
    assert_eq!(prompts(s).map(|p| p.text).collect::<Vec<_>>(), ["go"]);
    assert_eq!(notices(s, "p1").count(), 1);
    assert_eq!(
        resumes_of_all(s, &["p1", "c1"])
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [1, 0]
    );
    assert_eq!(children(s, &Owner::Agent("p1".into())).count(), 0); // c1 has no file
    assert_eq!(launches(s).count(), 0);
    assert_eq!(phases(s, "wf_none").count(), 0);
    let p1 = Owner::Agent("p1".into());
    assert_eq!(
        waiting(s, &p1, Ts(B + 30.0 * M))
            .map(|l| l.task.as_str())
            .collect::<Vec<_>>(),
        ["b1"]
    );
}
