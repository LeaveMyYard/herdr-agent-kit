//! herdr's event stream (`events.subscribe`), reconnecting: panorama
//! `herdr.events`.

mod stream;

use std::fmt;
use std::time::Duration;

use serde_json::{Map, Value};

use super::ids::{PaneId, TabId};

pub use stream::Events;

/// The first wait before a reconnect; it doubles up to [`MAX_BACKOFF`] and
/// starts over once herdr acknowledges or sends an event.
pub const FIRST_BACKOFF: Duration = Duration::from_millis(500);
/// The longest wait before a reconnect.
pub const MAX_BACKOFF: Duration = Duration::from_secs(10);

/// A subscription type (`tab.focused`). The constants are the ones the
/// plugins use; [`EventKind::new`] names any other from herdr's schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventKind(&'static str);

impl EventKind {
    /// A tab got the focus.
    pub const TAB_FOCUSED: Self = Self("tab.focused");
    /// A pane got the focus.
    pub const PANE_FOCUSED: Self = Self("pane.focused");
    /// A pane's agent changed state.
    pub const PANE_AGENT_STATUS_CHANGED: Self = Self("pane.agent_status_changed");

    /// Any subscription type herdr's schema names.
    pub const fn new(kind: &'static str) -> Self {
        Self(kind)
    }

    /// The type as herdr spells it.
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// One event's `data`: `{"type": "tab_focused", "tab_id", "workspace_id"}`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Event {
    /// `data.type` (`tab_focused`, `pane_focused`), or empty.
    pub kind: String,
    /// The whole `data` object.
    pub data: Map<String, Value>,
}

impl Event {
    fn str(&self, key: &str) -> Option<&str> {
        self.data.get(key).and_then(Value::as_str)
    }

    /// `data.pane_id`.
    pub fn pane_id(&self) -> Option<PaneId> {
        self.str("pane_id").map(PaneId::from)
    }

    /// `data.tab_id`.
    pub fn tab_id(&self) -> Option<TabId> {
        self.str("tab_id").map(TabId::from)
    }
}

/// What [`Events::next`] yields: herdr's events plus two markers of its own
/// (panorama's `_connected` and `_disconnected`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum HerdrEvent {
    /// herdr acknowledged the subscription (or sent an event) on a new
    /// connection: the stream is live. Events may have been missed while it
    /// was not.
    Connected,
    /// A live stream was lost; it reconnects by itself.
    Lost,
    /// An event.
    Event(Event),
}
