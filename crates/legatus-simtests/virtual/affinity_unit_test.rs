//! Story 143 unit tests: the table rules with a virtual clock and a fake availability.
use legatus_common::ids::{AliasName, NodeId};
use legatus_proxy::affinity::seams::*;
use legatus_proxy::affinity::table::*;
use legatus_proxy::key::harness::HarnessLabel;
use legatus_proxy::key::hasher::KEY_LEN_BYTES;
use legatus_proxy::key::{ConversationKey, KeyClass};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

const TTL: Duration = Duration::from_secs(600);
const MS: Duration = Duration::from_millis(1);
const SMALL_CAP: usize = 3;
const CANARY_PROMPT: &str = "CANARY-PROMPT-7f3a";

fn tk(alias: &str, n: u8) -> TableKey {
    TableKey { alias: AliasName(alias.to_string()), key: ConversationKey([n; KEY_LEN_BYTES]), credential: None }
}

fn node(name: &str) -> NodeId {
    NodeId(name.to_string())
}

fn table(cap: usize) -> Arc<AffinityTable> {
    Arc::new(AffinityTable::new(TableParams { ttl: TTL, cap, mature_turns: 2 }))
}

fn put(t: &AffinityTable, key: &TableKey, n: &str) -> PlaceOutcome {
    t.place(key.clone(), node(n), KeyClass::Strong, HarnessLabel::Known("pi".into()), Instant::now())
}

#[derive(Default)]
struct Fake(HashMap<String, UnavailableReason>);

impl NodeAvailability for Fake {
    fn is_available(&self, n: &NodeId) -> bool {
        !self.0.contains_key(&n.0)
    }
    fn unavailable_reason(&self, n: &NodeId) -> Option<UnavailableReason> {
        self.0.get(&n.0).copied()
    }
}

#[tokio::test(start_paused = true)]
async fn tc01_first_placement_stores_node_class_harness_and_the_placement_time_and_a_second_call_changes_nothing() {
    let t = table(10);
    let k = tk("x", 1);
    let at = Instant::now();
    assert_eq!(t.place(k.clone(), node("a"), KeyClass::Strong, HarnessLabel::Known("pi".into()), at), PlaceOutcome::Created);
    let (_, entry) = t.snapshot().pop().unwrap();
    assert_eq!((entry.node.0.as_str(), entry.last_seen, entry.in_flight, entry.done_count, entry.class), ("a", at, 0, 0, KeyClass::Strong));
    assert_eq!(entry.harness, HarnessLabel::Known("pi".into()));
    assert_eq!(put(&t, &k, "b"), PlaceOutcome::AlreadyPresent { node: node("a") });
    assert_eq!(t.peek(&k).unwrap().node, node("a"), "nothing changed");
    assert_eq!(t.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn tc03_and_tc08_the_entry_is_found_up_to_exactly_the_ttl_and_gone_one_millisecond_later() {
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    let start = Instant::now();
    let probe = |at: Duration| t.lookup(&k, start + at, &AllUp);
    assert_eq!(probe(TTL - MS), Lookup::Hit { node: node("a") });
    assert_eq!(probe(TTL), Lookup::Hit { node: node("a") }, "exactly at the expiry still finds it");
    assert_eq!(probe(TTL + MS), Lookup::Miss);
    assert!(t.peek(&k).is_none(), "an expired entry is removed by the lookup");
    assert_eq!(put(&t, &k, "b"), PlaceOutcome::Created, "placed as new after the expiry");
}

#[tokio::test(start_paused = true)]
async fn tc04_idle_200_seconds_past_the_protected_window_still_gives_a_hit_to_the_same_node() {
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    assert_eq!(t.lookup(&k, Instant::now() + Duration::from_secs(200), &AllUp), Lookup::Hit { node: node("a") });
}

#[tokio::test(start_paused = true)]
async fn tc05_at_the_cap_a_new_key_removes_the_oldest_idle_entry_and_the_size_stays_at_the_cap() {
    let t = table(SMALL_CAP);
    for n in 1..=3u8 {
        put(&t, &tk("x", n), "a");
        tokio::time::advance(Duration::from_secs(1)).await;
    }
    assert_eq!(t.len(), SMALL_CAP);
    assert_eq!(put(&t, &tk("x", 4), "a"), PlaceOutcome::Created);
    assert_eq!(t.len(), SMALL_CAP);
    assert!(t.peek(&tk("x", 1)).is_none(), "the oldest idle entry went");
    assert!(t.peek(&tk("x", 2)).is_some() && t.peek(&tk("x", 4)).is_some());
}

#[tokio::test(start_paused = true)]
async fn tc05_entries_placed_at_the_same_instant_leave_in_insertion_order() {
    let t = table(2);
    put(&t, &tk("x", 1), "a");
    put(&t, &tk("x", 2), "a");
    put(&t, &tk("x", 3), "a");
    assert!(t.peek(&tk("x", 1)).is_none() && t.peek(&tk("x", 2)).is_some());
}

#[tokio::test(start_paused = true)]
async fn tc06_at_the_cap_with_every_entry_running_nothing_is_removed_and_the_new_key_gets_no_entry() {
    let t = table(SMALL_CAP);
    let mut guards = Vec::new();
    for n in 1..=3u8 {
        put(&t, &tk("x", n), "a");
        guards.push(t.begin(&tk("x", n)).unwrap());
    }
    assert_eq!(t.skipped_full_count(), 0);
    assert_eq!(put(&t, &tk("x", 9), "a"), PlaceOutcome::SkippedFull);
    assert_eq!(t.skipped_full_count(), 1, "counted for the metric of story 138");
    assert_eq!(t.len(), SMALL_CAP);
    assert!(t.peek(&tk("x", 9)).is_none());
    assert!((1..=3u8).all(|n| t.peek(&tk("x", n)).is_some()), "no running entry was evicted");
    assert!(t.begin(&tk("x", 9)).is_none(), "a key with no entry has nothing to run on");
    drop(guards);
}

#[tokio::test(start_paused = true)]
async fn tc07_one_node_under_two_aliases_is_two_entries_and_both_are_counted() {
    let t = table(10);
    put(&t, &tk("coder", 1), "a");
    put(&t, &tk("chat", 1), "a");
    assert_eq!(t.len(), 2);
    assert_eq!(t.entries_on_node(&node("a")), 2);
    assert_eq!(t.entries_on_node(&node("b")), 0);
}

#[tokio::test(start_paused = true)]
async fn tc10_an_error_status_and_a_cut_stream_leave_last_seen_and_done_count_alone() {
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    let before = t.snapshot().pop().unwrap().1.last_seen;
    tokio::time::advance(Duration::from_secs(30)).await;
    let g = t.begin(&k).unwrap();
    assert_eq!(t.peek(&k).unwrap().in_flight, 1);
    g.finish(RequestEnd::Failed, Instant::now());
    let after = t.snapshot().pop().unwrap().1;
    assert_eq!((after.last_seen, after.done_count, after.in_flight, after.prev), (before, 0, 0, None));
}

#[tokio::test(start_paused = true)]
async fn tc11_a_complete_success_sets_last_seen_raises_done_count_and_stops_at_255() {
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    for turn in 1..=256u32 {
        tokio::time::advance(Duration::from_secs(1)).await;
        let g = t.begin(&k).unwrap();
        g.finish(RequestEnd::CompleteSuccess { prompt_tokens: 100, completion_tokens: 5 }, Instant::now());
        let entry = t.snapshot().pop().unwrap().1;
        assert_eq!(entry.done_count, u8::try_from(turn.min(255)).unwrap(), "turn {turn}");
        assert_eq!(entry.last_seen, Instant::now());
    }
}

#[tokio::test(start_paused = true)]
async fn tc12_a_request_running_longer_than_the_ttl_is_never_removed_and_idle_time_counts_from_the_end() {
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    let g = t.begin(&k).unwrap();
    tokio::time::advance(TTL * 3).await;
    assert_eq!(t.expire(Instant::now()), 0);
    assert_eq!(t.lookup(&k, Instant::now(), &AllUp), Lookup::Hit { node: node("a") });
    g.finish(RequestEnd::CompleteSuccess { prompt_tokens: 1, completion_tokens: 1 }, Instant::now());
    let ended = Instant::now();
    assert_eq!(t.lookup(&k, ended + TTL, &AllUp), Lookup::Hit { node: node("a") }, "idle time counts from the end of the response");
    assert_eq!(t.lookup(&k, ended + TTL + MS, &AllUp), Lookup::Miss);
}

#[tokio::test(start_paused = true)]
async fn tc16_an_unavailable_node_gives_unavailable_with_the_reason_and_leaves_the_entry_alone() {
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    for reason in [UnavailableReason::Down, UnavailableReason::Removed] {
        let fake = Fake(HashMap::from([("a".to_string(), reason)]));
        assert_eq!(t.lookup(&k, Instant::now(), &fake), Lookup::Unavailable { node: node("a"), reason });
    }
    assert_eq!(t.peek(&k).unwrap().node, node("a"));
    assert_eq!(t.lookup(&k, Instant::now(), &AllUp), Lookup::Hit { node: node("a") });
}

#[tokio::test(start_paused = true)]
async fn tc17_churn_of_ten_times_the_cap_never_exceeds_the_cap_and_running_entries_stay() {
    let t = table(SMALL_CAP);
    put(&t, &tk("x", 0), "a");
    let guard = t.begin(&tk("x", 0)).unwrap();
    for n in 1..=(10 * SMALL_CAP as u8) {
        put(&t, &tk("x", n), "a");
        assert!(t.len() <= SMALL_CAP, "after key {n}");
        tokio::time::advance(MS).await;
    }
    assert!(t.peek(&tk("x", 0)).is_some(), "the running entry stayed through the churn");
    drop(guard);
}

#[tokio::test(start_paused = true)]
async fn tc19_peek_and_paging_change_no_last_seen_and_give_the_four_fields() {
    let t = table(10);
    assert!(t.conversations_after(None, 10).is_empty(), "empty table");
    for n in 1..=5u8 {
        put(&t, &tk("x", n), "a");
    }
    let before: Vec<Instant> = t.snapshot().iter().map(|(_, e)| e.last_seen).collect();
    tokio::time::advance(Duration::from_secs(10)).await;
    let view = t.peek(&tk("x", 1)).unwrap();
    assert_eq!((view.node.0.as_str(), view.class, view.in_flight, view.done_count), ("a", KeyClass::Strong, 0, 0));
    let first = t.conversations_after(None, 2);
    assert_eq!(first.len(), 2);
    let second = t.conversations_after(Some(&first[1].0), 2);
    let third = t.conversations_after(Some(&second[1].0), 2);
    assert_eq!((second.len(), third.len()), (2, 1));
    let mut seen: Vec<_> = first.iter().chain(&second).chain(&third).map(|(k, _)| k.clone()).collect();
    seen.dedup();
    assert_eq!(seen.len(), 5, "paging covers each entry once");
    assert_eq!(t.conversations_after(None, 0).len(), 0);
    let after: Vec<Instant> = t.snapshot().iter().map(|(_, e)| e.last_seen).collect();
    assert_eq!(before, after);
}

#[tokio::test(start_paused = true)]
async fn tc20_a_complete_success_stores_the_turn_a_failure_keeps_the_old_one_and_confirmed_count_follows_done_count() {
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    assert_eq!((t.prev_turn(&k), t.confirmed_count()), (None, 0));
    t.begin(&k).unwrap().finish(RequestEnd::CompleteSuccess { prompt_tokens: 900, completion_tokens: 40 }, Instant::now());
    assert_eq!(t.prev_turn(&k), Some(PrevTurn { prompt_tokens: 900, completion_tokens: 40 }));
    assert_eq!(t.confirmed_count(), 0, "one turn is not yet mature");
    t.begin(&k).unwrap().finish(RequestEnd::Failed, Instant::now());
    assert_eq!(t.prev_turn(&k), Some(PrevTurn { prompt_tokens: 900, completion_tokens: 40 }));
    t.begin(&k).unwrap().finish(RequestEnd::CompleteSuccess { prompt_tokens: 950, completion_tokens: 10 }, Instant::now());
    assert_eq!(t.confirmed_count(), 1);
    assert_eq!(t.prev_turn(&k).unwrap().prompt_tokens, 950);
}

#[tokio::test(start_paused = true)]
async fn tc22_the_defaults_say_yes_and_the_table_gives_facts_for_a_key_and_none_for_an_absent_one() {
    let (up, on) = (AllUp, AlwaysOn);
    assert!(up.is_available(&node("any")) && up.unavailable_reason(&node("any")).is_none());
    assert!(on.mode_of(&node("any"), Instant::now()) && on.allows_entries(&node("any"), Instant::now()));
    on.set_initial(&node("any"), AffinityPays::Unknown, PaysReason::NotProbed);
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    let _g = t.begin(&k).unwrap();
    assert_eq!(t.facts(&node("a"), &k), Some(Facts { done_count: 0, class: KeyClass::Strong, in_flight: 1 }));
    assert_eq!(t.facts(&node("a"), &tk("x", 2)), None);
    assert_eq!(t.facts(&node("b"), &k), None, "another node is not this conversation's");
}

#[tokio::test(start_paused = true)]
async fn tc23_a_side_call_clears_in_flight_and_sets_last_seen_but_leaves_the_turn_and_done_count() {
    let t = table(10);
    let k = tk("x", 1);
    put(&t, &k, "a");
    t.begin(&k).unwrap().finish(RequestEnd::CompleteSuccess { prompt_tokens: 7, completion_tokens: 1 }, Instant::now());
    tokio::time::advance(Duration::from_secs(60)).await;
    t.begin(&k).unwrap().finish(RequestEnd::SideCallSuccess, Instant::now());
    let e = t.snapshot().pop().unwrap().1;
    assert_eq!((e.in_flight, e.done_count, e.prev, e.last_seen), (0, 1, Some(PrevTurn { prompt_tokens: 7, completion_tokens: 1 }), Instant::now()));
}

#[tokio::test(start_paused = true)]
async fn the_table_is_empty_when_made_and_no_debug_form_holds_a_raw_value() {
    let t = table(10);
    assert!(t.is_empty() && t.snapshot().is_empty());
    let key = tk("x", 7);
    put(&t, &key, "a");
    let shown = format!("{key:?} {:?} {:?}", t.snapshot(), t.peek(&key));
    assert!(shown.contains("0707070707070707"), "the hash as hex: {shown}");
    assert!(!format!("{shown}{CANARY_PROMPT}").replace(CANARY_PROMPT, "").contains("CANARY"));
    assert_eq!(format!("{:?}", CredentialHash([1; KEY_LEN_BYTES])), "CredentialHash(..)");
    // Parameters read back, and the catalogue defaults.
    assert_eq!(TableParams::default(), TableParams { ttl: Duration::from_secs(600), cap: 10_000, mature_turns: 2 });
    t.configure(TableParams { ttl: Duration::from_secs(5), cap: 1, mature_turns: 1 });
    assert_eq!(t.params().cap, 1);
}

#[tokio::test(start_paused = true)]
async fn next_due_is_one_millisecond_after_the_expiry_of_the_oldest_idle_entry_and_none_when_nothing_is_idle() {
    let t = table(10);
    assert_eq!(t.next_due(), None);
    let k = tk("x", 1);
    let at = Instant::now();
    put(&t, &k, "a");
    assert_eq!(t.next_due(), Some(at + TTL + MS));
    let g = t.begin(&k).unwrap();
    assert_eq!(t.next_due(), None, "a running entry is not due");
    drop(g);
    assert_eq!(t.expire(at + TTL), 0);
    assert_eq!(t.expire(at + TTL + MS), 1);
}

#[tokio::test(start_paused = true)]
async fn a_poisoned_lock_rebuilds_the_table_empty_and_writes_one_table_inconsistent_warning() {
    use legatus_proxy::obs::log_sink::LogRecord;
    use legatus_testkit::virt::MemorySink;
    let t = table(10);
    let sink = Arc::new(MemorySink::new(16));
    t.attach_sink(sink.clone());
    put(&t, &tk("x", 1), "a");
    t.poison_for_test();
    assert!(t.is_empty(), "a table that might be half-updated is not trusted");
    assert_eq!(put(&t, &tk("x", 2), "a"), PlaceOutcome::Created, "and it works again");
    assert_eq!(t.len(), 1);
    let warnings: Vec<_> = sink.take().into_iter().filter(|r| matches!(r, LogRecord::System(s) if s.code == Some("table_inconsistent"))).collect();
    assert_eq!(warnings.len(), 1);
}
