//! The read side, one view per concern. A view borrows only the stores it
//! reads; [`Session`] hands them out.

mod background;
mod conversation;
mod evidence;
mod parentage;
mod workflows;

pub use background::Background;
pub use conversation::Conversation;
pub use evidence::Evidence;
pub use parentage::Parentage;
pub use workflows::Workflows;

use super::Session;

/// The views.
impl Session {
    /// The main transcript's titles, prompts and continuation.
    pub fn conversation(&self) -> Conversation<'_> {
        Conversation {
            store: &self.conversation,
        }
    }

    /// The notices and resumes: what ended or reopened an agent or a task.
    pub fn evidence(&self) -> Evidence<'_> {
        Evidence {
            notices: &self.notices,
            resumes: &self.spawns.resumes,
        }
    }

    /// Who spawned whom: spawning calls, acknowledgements, parents,
    /// children.
    pub fn parentage(&self) -> Parentage<'_> {
        Parentage {
            agents: &self.agents,
            spawns: &self.spawns,
            children: &self.children,
        }
    }

    /// The workflow runs: launches, journals, summaries, how a run ended.
    pub fn workflows(&self) -> Workflows<'_> {
        Workflows {
            store: &self.workflows,
            notices: &self.notices,
        }
    }

    /// The background tasks, and which are still open or running.
    pub fn background(&self) -> Background<'_> {
        Background {
            store: &self.background,
            notices: &self.notices,
        }
    }
}
