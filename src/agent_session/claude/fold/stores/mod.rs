//! The keyed stores records are folded into, one per concern. Each store
//! holds what it is about and folds its own records; [`super::Session`]
//! routes records to them and the [views](super::views) read them.

use std::collections::{BTreeSet, HashMap};

use super::times::NoticeAt;

mod asks;
mod background;
mod conversation;
mod hooks;
mod spawns;
mod workflows;

pub(super) use asks::AskStore;
pub(super) use background::BgStore;
pub(super) use conversation::ConversationStore;
pub(super) use hooks::HookStore;
pub(super) use spawns::SpawnStore;
pub(super) use workflows::WorkflowStore;

/// The `<task-notification>`s by task id (an agent id, a background task
/// id or a workflow run's task id), from any carrier in any file.
pub(super) type Notices = HashMap<String, BTreeSet<NoticeAt>>;
