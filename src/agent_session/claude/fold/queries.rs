//! What the fold knows: agents, files, titles, prompts, notices, resumes
//! and question ids.

use std::collections::BTreeSet;

use super::times::max_opt;
use super::{AgentRec, FileFacts, Owner, Session, Titles};
use crate::agent_session::claude::wire::Ending;
use crate::agent_session::{SessionId, Ts};

/// What the fold knows: agents, files, titles, prompts, notices, resumes, question ids.
impl Session {
    /// The agents by id, in the order they were first recorded
    /// ([`Session::set_agent`] or [`Session::set_meta`]), as panorama's
    /// `Session.agents`.
    pub fn agents(&self) -> impl ExactSizeIterator<Item = (&str, &AgentRec)> + '_ {
        self.agents.iter().map(|(id, rec)| (id.as_str(), rec))
    }

    /// Agent `id`, once recorded.
    pub fn agent(&self, id: &str) -> Option<&AgentRec> {
        self.agents.get(id)
    }

    /// The facts of `owner`'s transcript, once it was read.
    pub fn file(&self, owner: &Owner) -> Option<&FileFacts> {
        self.files.get(owner)
    }

    /// The facts of agent `id`'s transcript.
    pub fn file_of(&self, id: &str) -> Option<&FileFacts> {
        self.agents.get(id)?;
        self.files.get(&Owner::Agent(id.to_string()))
    }

    /// The session's titles.
    pub fn titles(&self) -> &Titles {
        &self.titles
    }

    /// The session the conversation continued in (D6), if it did and was not
    /// resumed since. Forward only: nothing here names the sessions that
    /// continued **into** this one.
    pub fn continued_in(&self) -> Option<&SessionId> {
        self.continuation.next()
    }

    /// The human prompts of the main transcript, `(time, first line)`, in
    /// time order (equal ones once).
    pub fn prompts(&self) -> impl Iterator<Item = (Ts, &str)> {
        self.prompts.iter().map(|(t, s)| (Ts(t.0), s.as_str()))
    }

    /// The latest human prompt at or before `when`: its index among
    /// [`Session::prompts`] and its text (zoetrope's `prompt_for_ts`).
    pub fn prompt_before(&self, when: Option<Ts>) -> Option<(usize, &str)> {
        let when = when?.0;
        self.prompts
            .iter()
            .enumerate()
            .take_while(|(_, (t, _))| t.0 <= when)
            .last()
            .map(|(i, (_, s))| (i, s.as_str()))
    }

    /// The notices about `task`: `(time, ending, tool uses)`, oldest first.
    pub fn notices(&self, task: &str) -> Vec<(Ts, Ending, Option<u64>)> {
        self.notices
            .get(task)
            .map(|n| n.iter().map(|x| (Ts(x.0.0), x.1, x.2)).collect())
            .unwrap_or_default()
    }

    /// The times agent `id` was resumed by `SendMessage`, oldest first.
    pub fn resumes(&self, id: &str) -> Vec<Ts> {
        self.resumes
            .get(id)
            .map(|r| r.iter().map(|t| Ts(t.0)).collect())
            .unwrap_or_default()
    }

    /// The inbox question ids `owner`'s `ask_operator` calls recorded.
    pub fn qids(&self, owner: &Owner) -> Option<&BTreeSet<u64>> {
        self.qids.get(owner)
    }

    /// The newest time in agent `id`'s transcript (its tail's or its stream's).
    pub fn last_ts(&self, id: &str) -> Option<Ts> {
        let f = self.file_of(id)?;
        max_opt(f.info.last_ts.map(Ts::secs), f.last_ts.map(Ts::secs)).map(Ts)
    }

    /// The newest time in any file read: a prompt, a reply, a tool call, a
    /// notice.
    pub fn last_activity(&self) -> Option<Ts> {
        self.files
            .values()
            .flat_map(|f| [f.info.last_ts.map(Ts::secs), f.last_ts.map(Ts::secs)])
            .flatten()
            .reduce(f64::max)
            .map(Ts)
    }
}
