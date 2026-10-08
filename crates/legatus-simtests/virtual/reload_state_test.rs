//! Story 179 unit tests: the trigger, the swap, the signal merge, the pinned record, the result.
use super::reload_helpers::*;
use legatus_proxy::config::registry::ReloadResult;
use legatus_proxy::config::typed::Registry;
use legatus_testkit::faults::rules::SeededRng;
use std::sync::Arc;
use std::time::Duration;

const SCHEDULES: u64 = 1000;
const MAX_READERS: u64 = 4;
const READS_PER_READER: u64 = 6;
const BURST: usize = 100;

async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn tc01_one_trigger_with_a_changed_valid_file_swaps_to_generation_2() {
    let (h, trigger, _task) = running(V1);
    assert_eq!(h.state.get(), ReloadResult::NoneYet);
    h.source.set_text(V2);
    trigger.send();
    settle().await;
    let now = h.handle.snapshot();
    assert_eq!((now.generation, now.nodes.len(), now.aliases.len()), (2, 2, 2));
    match h.state.get() {
        ReloadResult::Loaded { generation, nodes, aliases, changed, .. } => assert_eq!((generation, nodes, aliases, changed), (2, 2, 2, true)),
        other => panic!("{other:?}"),
    }
    assert!(h.out.text().contains("registry reloaded: generation 2 (2 nodes, 2 aliases)"), "{}", h.out.text());
}

#[tokio::test(start_paused = true)]
async fn tc02_a_file_edit_without_a_trigger_reloads_nothing_however_long_we_wait() {
    let (h, _trigger, _task) = running(V1);
    h.source.set_text(V2);
    tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
    assert_eq!(h.handle.snapshot().generation, 1);
    assert_eq!(h.state.get(), ReloadResult::NoneYet);
    assert!(h.out.text().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc03_a_burst_of_100_triggers_during_one_reload_runs_exactly_one_follow_up_never_two_at_once() {
    let (h, trigger, _task) = running(V1);
    h.source.set_text(V2);
    let point = h.sim.arm("reload_apply").unwrap();
    trigger.send();
    point.wait_held(1).await;
    for _ in 0..BURST {
        trigger.send();
    }
    settle().await;
    assert_eq!(point.reached(), 1, "only one reload runs at a time");
    point.disarm();
    point.release_all();
    settle().await;
    let cycles = h.out.text().matches("registry reloaded:").count();
    assert_eq!(cycles, 2, "the first reload and one follow-up: {}", h.out.text());
    assert_eq!(h.handle.snapshot().generation, 2, "the follow-up found the same file and swapped nothing");
}

#[tokio::test(start_paused = true)]
async fn tc07_readers_see_a_whole_registry_under_1000_seeded_schedules() {
    for seed in 1..=SCHEDULES {
        let (h, trigger, task) = running(V1);
        h.source.set_text(V2);
        let point = h.sim.arm("reload_apply").unwrap();
        let mut rng = SeededRng::new(seed);
        let readers = 1 + rng.next_u64() % MAX_READERS;
        let release_after = rng.next_u64() % (readers * READS_PER_READER);
        trigger.send();
        point.wait_held(1).await;
        let mut tasks = Vec::new();
        for r in 0..readers {
            let handle = h.handle.clone();
            let yields: Vec<u64> = (0..READS_PER_READER).map(|_| rng.next_u64() % 3).collect();
            tasks.push(tokio::spawn(async move {
                let mut seen = Vec::new();
                for (i, y) in yields.into_iter().enumerate() {
                    for _ in 0..y {
                        tokio::task::yield_now().await;
                    }
                    let snapshot = handle.snapshot();
                    for alias in snapshot.aliases.iter() {
                        for node in &alias.nodes {
                            assert!(snapshot.node(node).is_some(), "seed {seed} reader {r} read {i}: alias {} names {node} which is not in generation {}", alias.name, snapshot.generation);
                        }
                    }
                    assert!(snapshot.generation == 1 || snapshot.generation == 2, "seed {seed}");
                    seen.push(snapshot.generation);
                }
                seen
            }));
        }
        for _ in 0..release_after {
            tokio::task::yield_now().await;
        }
        point.disarm();
        point.release_all();
        for t in tasks {
            let seen = t.await.unwrap();
            assert!(seen.windows(2).all(|w| w[0] <= w[1]), "seed {seed}: a reader went back in time {seen:?}");
        }
        settle().await;
        assert_eq!(h.handle.snapshot().generation, 2);
        task.abort();
    }
}

#[tokio::test(start_paused = true)]
async fn tc09_a_snapshot_taken_before_the_swap_keeps_its_own_nodes_and_is_freed_with_its_last_holder() {
    let h = harness(V1);
    let before = h.handle.snapshot();
    let weak = Arc::downgrade(&before);
    h.source.set_text(V2);
    h.reloader.reload_once().await;
    let after = h.handle.snapshot();
    assert_eq!((before.generation, before.nodes.len(), before.aliases.len()), (1, 1, 1), "the older record keeps its nodes");
    assert_eq!((after.generation, after.nodes.len()), (2, 2));
    assert!(weak.upgrade().is_some(), "still held by the request");
    drop(before);
    assert!(weak.upgrade().is_none(), "freed when its last holder ends");
}

#[tokio::test(start_paused = true)]
async fn tc16_after_a_swap_the_warning_list_is_the_one_of_the_new_file_only() {
    let with_warning = format!("{V1}").replace("engine: { name: ollama, version: \"0.35\" }", "engine: { name: mlx_lm }").replace("    model: model-a\n", "    model: model-a\n    responses: true\n");
    let h = harness(&with_warning);
    assert_eq!(h.handle.snapshot().warnings.len(), 1);
    h.source.set_text(V2);
    h.reloader.reload_once().await;
    assert!(h.handle.snapshot().warnings.is_empty());
    h.source.set_text(&with_warning);
    h.reloader.reload_once().await;
    assert_eq!(h.handle.snapshot().warnings.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn tc17_the_result_goes_none_yet_loaded_rejected_and_a_rejection_keeps_the_generation_and_takes_the_time_from_the_wall_clock() {
    let h = harness(V1);
    assert_eq!(h.state.get(), ReloadResult::NoneYet);
    h.source.set_text(V2);
    let first = h.reloader.reload_once().await;
    assert!(matches!(first, ReloadResult::Loaded { generation: 2, changed: true, .. }));
    tokio::time::sleep(Duration::from_secs(5)).await;
    h.source.set_text("version: 9\nnodes: {}\naliases: {}\n");
    match h.reloader.reload_once().await {
        ReloadResult::Rejected { at, errors } => {
            assert_eq!(at.unix_ms, 1_000 + 5_000, "the wall clock of the harness");
            assert!(errors.iter().any(|e| e.code.as_str() == "bad_version"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(h.handle.snapshot().generation, 2);
    assert!(matches!(h.state.get(), ReloadResult::Rejected { .. }));
    h.source.set_text(V1);
    assert!(matches!(h.reloader.reload_once().await, ReloadResult::Loaded { generation: 3, changed: true, .. }), "a valid file after a rejection loads");
}

#[tokio::test(start_paused = true)]
async fn the_swap_replaces_the_registry_as_one_value_and_the_handle_recovers_from_a_poisoned_lock() {
    let h = harness(V1);
    let handle = h.handle.clone();
    let poisoner = std::thread::spawn({
        let handle = handle.clone();
        move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _snapshot = handle.snapshot();
                panic!("a reader defect");
            }));
        }
    });
    poisoner.join().unwrap();
    h.source.set_text(V2);
    h.reloader.reload_once().await;
    let next: Arc<Registry> = h.handle.snapshot();
    assert_eq!(next.generation, 2);
}
