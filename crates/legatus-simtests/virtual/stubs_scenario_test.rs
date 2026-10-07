//! TC-01, TC-02, TC-04 of story 162: scenario parser, seed, rule limits.
use legatus_testkit::faults::RuleEngine;
use legatus_testkit::stubs::run::{arrival_times, run_in_memory};
use legatus_testkit::stubs::scenario::*;
use legatus_testkit::virt::virtual_runtime;
use std::time::Duration;

const THREE_STUBS: &str = "
scenario three
seed 7
stub ollama kind=serial_ollama max_queue=2 keep_alive_s=10
stub llama kind=multi_slot_llama slots=4 ctx_total=16384 metrics_on=true idle_close_s=2
stub mlx kind=batching_mlx wedge_after=3 cache_entries=10
fault llama action=status:503:30:busy limit=count:2
fault llama action=refuse limit=window:1000-2000
step 0 llama /v1/chat/completions 260
step 100 mlx /v1/chat/completions 260
";

#[test]
fn tc01_valid_scenario_parses_and_bad_ones_are_refused_before_any_start() {
    let s = parse_scenario(THREE_STUBS).unwrap();
    assert_eq!((s.name.as_str(), s.seed, s.stubs.len(), s.faults.len(), s.steps.len()), ("three", 7, 3, 2, 2));
    assert_eq!(s.stubs[0].max_queue, 2);
    assert_eq!(s.stubs[1].idle_close_s, 2);
    assert_eq!(s.faults[1].limit, Limit::Window { from_ms: 1000, to_ms: 2000 });
    assert_eq!(parse_scenario("stub a kind=quantum").err(), Some(ScenarioError::UnknownKind("quantum".into())));
    assert!(matches!(parse_scenario("stub a kind=serial_ollama bogus=1").err(), Some(ScenarioError::UnknownField(f)) if f == "stub.a.bogus"));
    assert!(matches!(parse_scenario("stub a kind=serial_ollama\nfault a action=refuse limit=count:0").err(), Some(ScenarioError::BadLimit(_))));
    assert!(matches!(parse_scenario("stub a kind=serial_ollama\nfault a action=refuse limit=window:500-500").err(), Some(ScenarioError::BadLimit(_))));
    assert!(matches!(parse_scenario("stub a kind=serial_ollama\nfault ghost action=refuse").err(), Some(ScenarioError::UnknownField(f)) if f.contains("ghost")));
    assert!(matches!(parse_scenario("stub a kind=serial_ollama speed=nope").err(), Some(ScenarioError::UnknownField(_))));
}

#[test]
fn tc02_same_seed_gives_identical_logs_20_of_20_and_another_seed_differs() {
    let text = "
scenario seeded
seed 11
stub llama kind=multi_slot_llama slots=4 load_time_ms=0
step 0 llama /v1/chat/completions 260
step 10 llama /v1/chat/completions 260
step 20 llama /v1/chat/completions 260
";
    let scenario = parse_scenario(text).unwrap();
    let mut runs = Vec::new();
    for _ in 0..20 {
        let rt = virtual_runtime();
        runs.push(rt.block_on(run_in_memory(&scenario)));
    }
    assert!(runs.iter().all(|r| *r == runs[0]));
    let mut other = scenario.clone();
    other.seed = 12;
    assert_ne!(arrival_times(&scenario), arrival_times(&other));
    let unseeded = Scenario { seed: 0, ..scenario.clone() };
    assert_eq!(arrival_times(&unseeded), vec![0, 10, 20], "no seed means no offsets");
}

#[tokio::test(start_paused = true)]
async fn tc04_limits_count_window_half_open_and_non_matching_never_fires() {
    let rule = |limit| FaultRule { stub: "s".into(), when: Match { path: Some("/a".into()), nth: None }, action: FaultAction::Refuse, limit };
    // Count fires n times, then stops.
    let engine = RuleEngine::new(vec![rule(Limit::Count(2))]);
    let fired: Vec<bool> = (0..4).map(|_| engine.decide("/a").is_some()).collect();
    assert_eq!(fired, vec![true, true, false, false]);
    // A request that does not match never fires (negative) and does not spend the count.
    let engine = RuleEngine::new(vec![rule(Limit::Count(1))]);
    assert!(engine.decide("/other").is_none());
    assert!(engine.decide("/a").is_some());
    // Window [1000, 2000) is half open at both edges.
    let engine = RuleEngine::new(vec![rule(Limit::Window { from_ms: 1000, to_ms: 2000 })]);
    let mut seen = Vec::new();
    for at in [999u64, 1000, 1999, 2000] {
        let now = engine.elapsed_ms();
        tokio::time::sleep(Duration::from_millis(at - now)).await;
        seen.push((at, engine.decide("/a").is_some()));
    }
    assert_eq!(seen, vec![(999, false), (1000, true), (1999, true), (2000, false)]);
    // nth picks only the nth matching request; Always never closes.
    let engine = RuleEngine::new(vec![FaultRule { when: Match { path: None, nth: Some(3) }, ..rule(Limit::Always) }]);
    let hits: Vec<bool> = (0..5).map(|_| engine.decide("/x").is_some()).collect();
    assert_eq!(hits, vec![false, false, true, false, false]);
    let always = RuleEngine::new(vec![rule(Limit::Always)]);
    assert!((0..10).all(|_| always.decide("/a").is_some()));
}
