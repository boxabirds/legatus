//! TC-16 to TC-20: named pause points.
use legatus_proxy::sim::{sim_point, SimError, SimPointName, SimPoints};
use std::sync::{Arc, Mutex};

#[tokio::test(start_paused = true)]
async fn tc16_unarmed_point_returns_at_once_and_counts_nothing() {
    let sim = SimPoints::new();
    sim_point(&sim, SimPointName::SlotFree).await;
    let controller = sim.arm("slot_free").unwrap();
    controller.disarm();
    sim_point(&sim, SimPointName::SlotFree).await;
    assert_eq!(controller.reached(), 0);
}

#[tokio::test(start_paused = true)]
async fn tc17_arm_after_zero_holds_every_arrival_including_an_earlier_sweep() {
    let sim = SimPoints::new();
    let controller = sim.arm("sweep_after_due").unwrap();
    let done = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for id in 0..2u32 {
        let (sim, done) = (sim.clone(), done.clone());
        handles.push(tokio::spawn(async move {
            sim_point(&sim, SimPointName::SweepAfterDue).await;
            done.lock().unwrap().push(id);
        }));
    }
    controller.wait_held(2).await;
    assert!(done.lock().unwrap().is_empty(), "nothing may pass an armed point");
    controller.release_all();
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(done.lock().unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn tc18_arm_after_two_lets_two_pass_and_holds_the_third() {
    let sim = SimPoints::new();
    let controller = sim.arm("admit_decide").unwrap();
    controller.arm_after(2);
    let done = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for id in 0..3u32 {
        let (sim, done) = (sim.clone(), done.clone());
        handles.push(tokio::spawn(async move {
            sim_point(&sim, SimPointName::AdmitDecide).await;
            done.lock().unwrap().push(id);
        }));
        tokio::task::yield_now().await;
    }
    controller.wait_held(1).await;
    assert_eq!(*done.lock().unwrap(), vec![0, 1]);
    assert_eq!(controller.reached(), 3);
    controller.release(0);
    handles.pop().unwrap().await.unwrap();
    assert_eq!(*done.lock().unwrap(), vec![0, 1, 2]);
}

#[test]
fn tc19_unknown_name_lists_valid_names_and_release_of_a_free_index_does_nothing() {
    let sim = SimPoints::new();
    let err = sim.arm("nope").err().unwrap();
    assert_eq!(err, SimError::UnknownSimPoint("nope".into()));
    let text = err.to_string();
    for name in ["table_lookup", "admit_decide", "slot_free", "hold_expire", "stream_end", "log_append", "sweep_after_due", "reload_apply"] {
        assert!(text.contains(name), "{text}");
    }
    let controller = sim.arm("stream_end").unwrap();
    controller.release(7);
    assert_eq!(controller.reached(), 0);
}

#[tokio::test(start_paused = true)]
async fn tc20_forced_interleaving_gives_a_first_in_one_run_and_b_first_in_another() {
    async fn run(release_a_first: bool) -> Vec<&'static str> {
        let sim = SimPoints::new();
        let controller = sim.arm("sweep_after_due").unwrap();
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for name in ["A", "B"] {
            let (sim, order) = (sim.clone(), order.clone());
            handles.push(tokio::spawn(async move {
                sim_point(&sim, SimPointName::SweepAfterDue).await;
                order.lock().unwrap().push(name);
            }));
            tokio::task::yield_now().await;
        }
        controller.wait_held(2).await;
        let (first, second) = if release_a_first { (0, 1) } else { (1, 0) };
        controller.release(first);
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        controller.release(second);
        for h in handles {
            h.await.unwrap();
        }
        let result = order.lock().unwrap().clone();
        result
    }
    assert_eq!(run(true).await, vec!["A", "B"]);
    assert_eq!(run(false).await, vec!["B", "A"]);
    assert_eq!(run(true).await, vec!["A", "B"], "repeatable");
}
