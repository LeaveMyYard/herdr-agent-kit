//! The session's agents and what their files tell: records, transcript
//! facts, last activity (and, plugin-only, the inbox question ids).

use super::times::max_opt;
use super::{AgentRec, FileFacts, Owner, Session};
use crate::agent_session::Ts;

/// The agents and their files.
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

    /// The inbox question ids `owner`'s `ask_operator` calls recorded.
    ///
    /// **Plugin-only** (feature `internal`, hidden from the docs, outside
    /// semver, like [`crate::internal`]): the `ask_operator` tool and its
    /// receipt are herdr-inbox's, not Claude Code's.
    #[cfg(feature = "internal")]
    #[doc(hidden)]
    pub fn qids(&self, owner: &Owner) -> Option<&std::collections::BTreeSet<u64>> {
        self.asks.qids.get(owner)
    }
}
