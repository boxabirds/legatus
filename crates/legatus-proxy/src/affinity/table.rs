//! The affinity table (contract C34): given an alias and a key, which node served this
//! conversation last. Soft state in memory: empty at start, nothing written, no text and no raw
//! session value (the key is a keyed hash). It does not decide whether a node has room or whether
//! to move; that belongs to admission, moves and the affinity mode.
use crate::config::settings::table_defaults;
use crate::affinity::seams::{NodeAvailability, UnavailableReason};
use crate::key::harness::HarnessLabel;
use crate::key::hasher::KEY_LEN_BYTES;
use crate::key::{ConversationKey, KeyClass};
use legatus_common::ids::{AliasName, NodeId};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

/// `done_count` stops here (it is a `u8`, so the saturating add holds it).
pub const MAX_DONE_COUNT: u8 = u8::MAX;
/// An entry is due for removal this long after its expiry instant: it is expired when more than
/// `ttl` has passed, and the clock has millisecond steps in the specification.
pub const EXPIRY_STEP: Duration = Duration::from_millis(1);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CredentialHash(pub [u8; KEY_LEN_BYTES]);

impl std::fmt::Debug for CredentialHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialHash(..)")
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TableKey {
    pub alias: AliasName,
    pub key: ConversationKey,
    pub credential: Option<CredentialHash>,
}

impl std::fmt::Debug for TableKey {
    /// The hash as hex, never a raw value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hex: String = self.key.0.iter().map(|b| format!("{b:02x}")).collect();
        write!(f, "TableKey({}, {hex})", self.alias.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrevTurn {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub node: NodeId,
    pub last_seen: Instant,
    pub in_flight: u32,
    pub done_count: u8,
    pub class: KeyClass,
    pub harness: HarnessLabel,
    pub prev: Option<PrevTurn>,
    /// Position in the idle index while no request runs.
    idle_at: Option<(Instant, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryView {
    pub node: NodeId,
    pub class: KeyClass,
    pub in_flight: u32,
    pub done_count: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableParams {
    pub ttl: Duration,
    pub cap: usize,
    pub mature_turns: u8,
}

impl Default for TableParams {
    fn default() -> Self {
        // The defaults live in the settings catalogue (story 165) and nowhere else.
        let (ttl_s, cap, mature) = table_defaults();
        TableParams { ttl: Duration::from_secs(ttl_s), cap: usize::try_from(cap).unwrap_or(usize::MAX), mature_turns: u8::try_from(mature).unwrap_or(u8::MAX) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    Hit { node: NodeId },
    Unavailable { node: NodeId, reason: UnavailableReason },
    Miss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceOutcome {
    Created,
    AlreadyPresent { node: NodeId },
    SkippedFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestEnd {
    CompleteSuccess { prompt_tokens: u32, completion_tokens: u32 },
    SideCallSuccess,
    Failed,
}

#[derive(Default)]
struct Inner {
    map: HashMap<TableKey, Entry>,
    /// Entries with no request running, oldest first. The sequence number breaks ties by insertion.
    idle: BTreeMap<(Instant, u64), TableKey>,
    seq: u64,
}

impl Inner {
    fn mark_idle(&mut self, key: &TableKey) {
        self.seq += 1;
        let seq = self.seq;
        if let Some(entry) = self.map.get_mut(key) {
            entry.idle_at = Some((entry.last_seen, seq));
            self.idle.insert((entry.last_seen, seq), key.clone());
        }
    }

    fn unmark_idle(&mut self, key: &TableKey) {
        if let Some(at) = self.map.get_mut(key).and_then(|e| e.idle_at.take()) {
            self.idle.remove(&at);
        }
    }

    fn remove(&mut self, key: &TableKey) {
        self.unmark_idle(key);
        self.map.remove(key);
    }
}

pub struct AffinityTable {
    params: Mutex<TableParams>,
    inner: Mutex<Inner>,
    wake: Notify,
}

fn expired(entry: &Entry, now: Instant, ttl: Duration) -> bool {
    entry.in_flight == 0 && now.saturating_duration_since(entry.last_seen) > ttl
}

impl AffinityTable {
    pub fn new(params: TableParams) -> AffinityTable {
        AffinityTable { params: Mutex::new(params), inner: Mutex::new(Inner::default()), wake: Notify::new() }
    }

    pub fn params(&self) -> TableParams {
        self.params.lock().map(|p| *p).unwrap_or_default()
    }

    /// Set the parameters once the registry is loaded (the settings `table_ttl_s`, `table_cap`
    /// and `mature_turns`). They are fixed from then on: a reload that changes them waits for a
    /// restart (story 179).
    pub fn configure(&self, params: TableParams) {
        if let Ok(mut held) = self.params.lock() {
            *held = params;
        }
    }

    /// The lock; a poisoned one is rebuilt empty, because a half-updated table cannot be trusted.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| {
            let mut guard = poisoned.into_inner();
            *guard = Inner::default();
            self.inner.clear_poison();
            guard
        })
    }

    /// Wakes the sweeper when an entry becomes idle with a due time it may not know.
    pub fn woken(&self) -> &Notify {
        &self.wake
    }

    /// An unexpired entry gives `Hit` when its node is available and `Unavailable` otherwise; an
    /// expired entry is removed and gives `Miss`, whether or not the sweeper ran first. Never
    /// changes `last_seen`, `in_flight` or `done_count`. A full node is available.
    pub fn lookup(&self, k: &TableKey, now: Instant, avail: &dyn NodeAvailability) -> Lookup {
        let mut inner = self.lock();
        let Some(entry) = inner.map.get(k) else { return Lookup::Miss };
        if expired(entry, now, self.params().ttl) {
            inner.remove(k);
            return Lookup::Miss;
        }
        let node = entry.node.clone();
        if avail.is_available(&node) {
            Lookup::Hit { node }
        } else {
            let reason = avail.unavailable_reason(&node).unwrap_or(UnavailableReason::Down);
            Lookup::Unavailable { node, reason }
        }
    }

    /// A read-only view: no expiry check, no availability check, no change.
    pub fn peek(&self, k: &TableKey) -> Option<EntryView> {
        self.lock().map.get(k).map(view_of)
    }

    /// Create the entry of a new key. At the cap the oldest idle entry (ties by insertion) is
    /// removed first; when every entry is running the key is served without an entry.
    pub fn place(&self, k: TableKey, node: NodeId, class: KeyClass, harness: HarnessLabel, now: Instant) -> PlaceOutcome {
        let mut inner = self.lock();
        if let Some(existing) = inner.map.get(&k) {
            return PlaceOutcome::AlreadyPresent { node: existing.node.clone() };
        }
        if inner.map.len() >= self.params().cap {
            let Some(oldest) = inner.idle.values().next().cloned() else { return PlaceOutcome::SkippedFull };
            inner.remove(&oldest);
        }
        inner.map.insert(k.clone(), Entry { node, last_seen: now, in_flight: 0, done_count: 0, class, harness, prev: None, idle_at: None });
        inner.mark_idle(&k);
        drop(inner);
        self.wake.notify_one();
        PlaceOutcome::Created
    }

    /// A request on this key begins: `in_flight` goes up and the entry leaves the idle index.
    /// `None` when the key has no entry (after `SkippedFull`, say).
    pub fn begin(self: &Arc<Self>, k: &TableKey) -> Option<InFlight> {
        let mut inner = self.lock();
        inner.unmark_idle(k);
        let entry = inner.map.get_mut(k)?;
        entry.in_flight += 1;
        Some(InFlight { table: self.clone(), key: k.clone(), finished: false })
    }

    fn end(&self, k: &TableKey, end: RequestEnd, now: Instant) {
        let mut inner = self.lock();
        let Some(entry) = inner.map.get_mut(k) else { return };
        entry.in_flight = entry.in_flight.saturating_sub(1);
        match end {
            RequestEnd::CompleteSuccess { prompt_tokens, completion_tokens } => {
                entry.last_seen = now;
                entry.done_count = entry.done_count.saturating_add(1);
                entry.prev = Some(PrevTurn { prompt_tokens, completion_tokens });
            }
            RequestEnd::SideCallSuccess => entry.last_seen = now,
            RequestEnd::Failed => {}
        }
        if entry.in_flight == 0 {
            inner.mark_idle(k);
            drop(inner);
            self.wake.notify_one();
        }
    }

    /// The stored turn of a conversation (read-only, for story 171).
    pub fn prev_turn(&self, k: &TableKey) -> Option<PrevTurn> {
        self.lock().map.get(k).and_then(|e| e.prev)
    }

    /// Entries that have completed at least `mature_turns` turns.
    pub fn confirmed_count(&self) -> usize {
        let mature = self.params().mature_turns;
        self.lock().map.values().filter(|e| e.done_count >= mature).count()
    }

    /// Remove every expired entry; the number removed.
    pub fn expire(&self, now: Instant) -> usize {
        let mut inner = self.lock();
        let mut removed = 0;
        let ttl = self.params().ttl;
        while let Some((&(last_seen, _), key)) = inner.idle.iter().next() {
            if now.saturating_duration_since(last_seen) <= ttl {
                break;
            }
            let key = key.clone();
            inner.remove(&key);
            removed += 1;
        }
        removed
    }

    /// The instant the oldest idle entry is first expired, or none when nothing is idle.
    pub fn next_due(&self) -> Option<Instant> {
        self.lock().idle.keys().next().map(|(last_seen, _)| *last_seen + self.params().ttl + EXPIRY_STEP)
    }

    /// Copies of every entry; changes nothing.
    pub fn snapshot(&self) -> Vec<(TableKey, Entry)> {
        let inner = self.lock();
        let mut all: Vec<(TableKey, Entry)> = inner.map.iter().map(|(k, e)| (k.clone(), e.clone())).collect();
        all.sort_by(|a, b| order_key(&a.0).cmp(&order_key(&b.0)));
        all
    }

    /// A page of conversations after `after`, in a fixed order (alias, then key bytes).
    pub fn conversations_after(&self, after: Option<&TableKey>, limit: usize) -> Vec<(TableKey, EntryView)> {
        let inner = self.lock();
        let mut all: Vec<(&TableKey, &Entry)> = inner.map.iter().collect();
        all.sort_by(|a, b| order_key(a.0).cmp(&order_key(b.0)));
        let start = match after {
            Some(a) => all.iter().position(|(k, _)| order_key(k) > order_key(a)).unwrap_or(all.len()),
            None => 0,
        };
        all[start..].iter().take(limit).map(|(k, e)| ((*k).clone(), view_of(e))).collect()
    }

    pub fn entries_on_node(&self, node: &NodeId) -> usize {
        self.lock().map.values().filter(|e| &e.node == node).count()
    }

    pub fn len(&self) -> usize {
        self.lock().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn order_key(k: &TableKey) -> (String, [u8; KEY_LEN_BYTES], Option<[u8; KEY_LEN_BYTES]>) {
    (k.alias.0.clone(), k.key.0, k.credential.map(|c| c.0))
}

fn view_of(e: &Entry) -> EntryView {
    EntryView { node: e.node.clone(), class: e.class, in_flight: e.in_flight, done_count: e.done_count }
}

/// One request that is running on an entry. Dropping it without `finish` (a cancel, a
/// disconnect) lowers `in_flight` at once and changes nothing else.
pub struct InFlight {
    table: Arc<AffinityTable>,
    key: TableKey,
    finished: bool,
}

impl InFlight {
    pub fn finish(mut self, end: RequestEnd, now: Instant) {
        self.finished = true;
        self.table.end(&self.key, end, now);
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        if !self.finished {
            self.table.end(&self.key, RequestEnd::Failed, Instant::now());
        }
    }
}
