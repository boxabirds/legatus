//! The traits the affinity table consumes and later stories implement, each with a default that
//! lets the table stand alone: every node up, affinity always on.
use crate::affinity::table::{AffinityTable, TableKey};
use crate::key::KeyClass;
use legatus_common::ids::NodeId;
use tokio::time::Instant;

/// Whether affinity is switched on for a node (the live source is story 194).
pub trait AffinityMode {
    fn is_on(&self) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AffinityPays {
    True,
    False,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaysReason {
    Measured,
    NoReuseField,
    NotProbed,
}

/// Whether entries are allowed on a node at a given time. `now` is passed so a re-probe needs no
/// clock of its own.
pub trait NodeModes: Send + Sync {
    fn mode_of(&self, node: &NodeId, now: Instant) -> bool;
    fn allows_entries(&self, node: &NodeId, now: Instant) -> bool;
    fn set_initial(&self, node: &NodeId, pays: AffinityPays, reason: PaysReason);
}

/// The default: affinity is on for every node.
pub struct AlwaysOn;

impl NodeModes for AlwaysOn {
    fn mode_of(&self, _node: &NodeId, _now: Instant) -> bool {
        true
    }
    fn allows_entries(&self, _node: &NodeId, _now: Instant) -> bool {
        true
    }
    fn set_initial(&self, _node: &NodeId, _pays: AffinityPays, _reason: PaysReason) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    Down,
    Removed,
}

/// Can the node of an entry take a request? A full node is available. A node the registry does not
/// know is not.
pub trait NodeAvailability: Send + Sync {
    fn is_available(&self, node: &NodeId) -> bool;
    fn unavailable_reason(&self, node: &NodeId) -> Option<UnavailableReason>;
}

/// The default: every node is available.
pub struct AllUp;

impl NodeAvailability for AllUp {
    fn is_available(&self, _node: &NodeId) -> bool {
        true
    }
    fn unavailable_reason(&self, _node: &NodeId) -> Option<UnavailableReason> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    pub done_count: u8,
    pub class: KeyClass,
    pub in_flight: u32,
}

/// What is known about one conversation on one node.
pub trait ConversationFacts {
    fn facts(&self, node: &NodeId, key: &TableKey) -> Option<Facts>;
}

impl ConversationFacts for AffinityTable {
    fn facts(&self, node: &NodeId, key: &TableKey) -> Option<Facts> {
        let view = self.peek(key)?;
        (&view.node == node).then_some(Facts { done_count: view.done_count, class: view.class, in_flight: view.in_flight })
    }
}
