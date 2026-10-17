//! The Claude-side producer of the neutral [`SessionView`]. **Pure.**

use super::{Hop, Session, SessionId};
use crate::agent_session::{SessionView, Status};

impl Session {
    /// The neutral view of this session, for a plugin's core.
    ///
    /// What Claude Code's files say is computed here:
    ///
    /// - `title`: the first non-empty of the `custom-title`, `agent-name` and
    ///   `ai-title` records ([`Session::titles`]), in that order, as
    ///   panorama's `collect.py` picks a label;
    /// - `successors`: the hops of `chain` after this session, each with the
    ///   time of the continuation that led to it ([`Hop::at`]). `chain` is
    ///   [`super::chain`] from this session's id; when it does not contain
    ///   that id, there are no successors. A last hop whose transcript is not
    ///   found yet is still a successor.
    ///
    /// What the files do not hold comes from the caller: `status` is the
    /// session's own status (the fold computes the statuses of its subagents,
    /// [`Session::status_of`], not of the session), and `parent` and
    /// `children` are the sessions it was started from and started, which the
    /// process tree tells ([`crate::agent_procs`]), not the transcripts.
    pub fn view(
        &self,
        status: Status,
        parent: Option<SessionId>,
        children: Vec<SessionId>,
        chain: &[Hop],
    ) -> SessionView {
        let titles = self.titles();
        let title = [&titles.custom, &titles.agent, &titles.ai]
            .into_iter()
            .flatten()
            .find(|t| !t.is_empty())
            .cloned();
        let successors = chain
            .iter()
            .skip_while(|h| h.id != *self.id())
            .skip(1)
            .map(|h| (h.id.clone(), h.at))
            .collect();
        SessionView {
            id: self.id().clone(),
            status,
            parent,
            children,
            title,
            successors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_session::Ts;
    use crate::agent_session::claude::Line;
    use crate::agent_session::claude::fold::Owner;

    const OWN: &str = "00000000-0000-4000-8000-000000000001";
    const NEXT: &str = "00000000-0000-4000-8000-000000000002";
    const LAST: &str = "00000000-0000-4000-8000-000000000003";

    fn id(s: &str) -> SessionId {
        SessionId::parse(s).unwrap()
    }

    fn feed(s: &mut Session, raw: &str) {
        let line = Line::parse(raw.as_bytes()).unwrap();
        let ts = line.ts();
        s.apply(Owner::Main, &line, ts);
    }

    #[test]
    fn title_takes_the_first_non_empty_of_custom_agent_ai() {
        let mut s = Session::new(id(OWN));
        let v = s.view(Status::Run, None, vec![], &[]);
        assert_eq!(v.title, None);
        feed(&mut s, r#"{"type":"ai-title","aiTitle":"ai"}"#);
        feed(&mut s, r#"{"type":"agent-name","agentName":"agent"}"#);
        assert_eq!(
            s.view(Status::Run, None, vec![], &[]).title.as_deref(),
            Some("agent")
        );
        feed(&mut s, r#"{"type":"custom-title","customTitle":""}"#);
        assert_eq!(
            s.view(Status::Run, None, vec![], &[]).title.as_deref(),
            Some("agent")
        );
        feed(&mut s, r#"{"type":"custom-title","customTitle":"mine"}"#);
        assert_eq!(
            s.view(Status::Run, None, vec![], &[]).title.as_deref(),
            Some("mine")
        );
    }

    #[test]
    fn successors_are_the_hops_after_the_session_with_their_times() {
        let s = Session::new(id(OWN));
        let chain = [
            Hop {
                id: id(OWN),
                found: true,
                at: None,
            },
            Hop {
                id: id(NEXT),
                found: true,
                at: Some(Ts(10.0)),
            },
            Hop {
                id: id(LAST),
                found: false,
                at: None,
            },
        ];
        let v = s.view(Status::Done, Some(id(LAST)), vec![id(NEXT)], &chain);
        assert_eq!(v.id, id(OWN));
        assert_eq!(v.status, Status::Done);
        assert_eq!(v.parent, Some(id(LAST)));
        assert_eq!(v.children, vec![id(NEXT)]);
        assert_eq!(
            v.successors,
            vec![(id(NEXT), Some(Ts(10.0))), (id(LAST), None)]
        );
        let other = [Hop {
            id: id(NEXT),
            found: true,
            at: None,
        }];
        assert!(
            s.view(Status::Done, None, vec![], &other)
                .successors
                .is_empty()
        );
    }
}
