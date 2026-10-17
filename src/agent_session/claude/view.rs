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
mod tests;
