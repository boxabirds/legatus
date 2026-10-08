//! Story 143 task 1: the idle-return gaps measured on real Claude Code transcripts are replayed
//! through the expiry rule of the real table, and the counts must equal the ones the measuring
//! script printed. The gaps are numbers only; the source is the owner machine, not harness
//! traffic through the proxy, so the 600 s default stays PROPOSED.
use legatus_common::ids::{AliasName, NodeId};
use legatus_proxy::affinity::seams::AllUp;
use legatus_proxy::affinity::table::{AffinityTable, Lookup, TableKey, TableParams};
use legatus_proxy::key::harness::HarnessLabel;
use legatus_proxy::key::hasher::KEY_LEN_BYTES;
use legatus_proxy::key::{ConversationKey, KeyClass};
use std::time::Duration;
use tokio::time::Instant;

const GAPS: &str = include_str!("../../../specs/proxy/evidence/ttl/idle-return-gaps-20261008T014715Z.txt");
const REPORT: &str = include_str!("../../../specs/proxy/evidence/ttl/ttl-replay-20261008T014715Z.txt");
const TTLS: [u64; 4] = [300, 600, 3600, 86400];

fn gaps() -> Vec<f64> {
    GAPS.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).map(|l| l.parse().unwrap()).collect()
}

fn found_through_the_table(gaps: &[f64], ttl: u64) -> usize {
    let table = AffinityTable::new(TableParams { ttl: Duration::from_secs(ttl), cap: gaps.len() + 1, mature_turns: 2 });
    let start = Instant::now();
    let mut found = 0;
    for (i, gap) in gaps.iter().enumerate() {
        let n = u32::try_from(i).unwrap().to_le_bytes();
        let mut bytes = [0u8; KEY_LEN_BYTES];
        bytes[..4].copy_from_slice(&n);
        let key = TableKey { alias: AliasName("x".into()), key: ConversationKey(bytes), credential: None };
        table.place(key.clone(), NodeId("a".into()), KeyClass::Strong, HarnessLabel::Unknown, start);
        // The script prints gaps to one decimal; the table counts whole milliseconds.
        let at = start + Duration::from_millis((gap * 1000.0).round() as u64);
        if matches!(table.lookup(&key, at, &AllUp), Lookup::Hit { .. }) {
            found += 1;
        }
    }
    found
}

#[tokio::test(start_paused = true)]
async fn the_table_finds_the_same_number_of_returns_as_the_measuring_script_for_each_ttl() {
    let gaps = gaps();
    assert_eq!(gaps.len(), 13968);
    let rows: Vec<Vec<&str>> = REPORT.lines().filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_digit()) && l.split_whitespace().count() == 7).map(|l| l.split_whitespace().collect()).collect();
    assert_eq!(rows.len(), TTLS.len());
    for (ttl, row) in TTLS.iter().zip(&rows) {
        assert_eq!(row[0].parse::<u64>().unwrap(), *ttl);
        let printed: usize = row[1].parse().unwrap();
        let by_table = found_through_the_table(&gaps, *ttl);
        // Rounding the gap to a millisecond can move a gap that sits within half a millisecond of the ttl.
        assert!(by_table.abs_diff(printed) <= 1, "ttl {ttl}: table {by_table}, script {printed}");
    }
}

#[test]
fn the_default_of_600_seconds_finds_most_returns_and_the_numbers_say_how_many() {
    let gaps = gaps();
    let found = |ttl: f64| gaps.iter().filter(|g| **g <= ttl).count();
    let percent = |n: usize| 100.0 * n as f64 / gaps.len() as f64;
    assert!((percent(found(600.0)) - 86.2).abs() < 0.1);
    assert!(percent(found(300.0)) < percent(found(600.0)) && percent(found(600.0)) < percent(found(3600.0)));
    // 600 s is not shown to be right: a longer value finds more returns at a higher memory cost.
    assert!(percent(found(3600.0)) - percent(found(600.0)) > 5.0);
}
