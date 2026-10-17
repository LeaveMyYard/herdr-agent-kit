//! What herdr answers for an action: a move, a swap, a notification.

use serde::{Deserialize, Serialize};

use super::Extra;
use super::lenient::lenient;
use super::snapshot::PaneInfo;
use crate::herdr_api::ids::PaneId;

/// `pane.move`'s `move_result`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MoveResult {
    /// Whether anything moved.
    #[serde(default, deserialize_with = "lenient")]
    pub changed: bool,
    /// The pane's id before the move.
    #[serde(default, deserialize_with = "lenient")]
    pub previous_pane_id: Option<PaneId>,
    /// The pane after the move: its `pane_id` is the pane's id from now on.
    #[serde(default, deserialize_with = "lenient")]
    pub pane: Option<PaneInfo>,
    /// Everything else (`target_layout`, ...).
    #[serde(flatten)]
    pub extra: Extra,
}

/// `pane.swap`'s `swap`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SwapResult {
    /// Whether the panes were swapped. herdr says `false` with a `reason`
    /// (`cross_tab`) instead of an error.
    #[serde(default, deserialize_with = "lenient")]
    pub changed: Option<bool>,
    /// Why not.
    #[serde(default, deserialize_with = "lenient")]
    pub reason: Option<String>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

impl SwapResult {
    /// panorama `swap_panes`: swapped unless herdr said `changed: false`.
    pub fn swapped(&self) -> bool {
        self.changed.unwrap_or(true)
    }
}

/// `notification.show`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NotificationResult {
    /// Whether it was shown.
    #[serde(default, deserialize_with = "lenient")]
    pub shown: Option<bool>,
    /// Why not (`disabled`).
    #[serde(default, deserialize_with = "lenient")]
    pub reason: Option<String>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}
