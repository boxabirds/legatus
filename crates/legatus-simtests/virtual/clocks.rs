//! TC-01 to TC-05: clocks, driver surface, offsets, ordering.
use legatus_proxy::sorted::sorted_keys;
use legatus_proxy::time::{Instant, WallClock, WallTime};
use legatus_testkit::virt::{virtual_runtime, Driver, MsOffset, OffsetError, SimWall};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const ONE_HOUR: Duration = Duration::from_secs(3600);
const TOUCH_EVERY: Duration = Duration::from_secs(600);
const IDLE_LIMIT: Duration = Duration::from_secs(1800);
const DRIVER_STEP: Duration = Duration::from_secs(1);

#[tokio::test(start_paused = true)]
async fn tc01_one_hour_keeps_a_touched_timer_alive() {
    let started = Instant::now();
    let mut last_touch = started;
    let mut elapsed = Duration::ZERO;
    while elapsed < ONE_HOUR {
        Driver::run_for(TOUCH_EVERY, DRIVER_STEP).await;
        elapsed += TOUCH_EVERY;
        assert!(Instant::now() - last_touch < IDLE_LIMIT, "idle limit reached at {elapsed:?}");
        last_touch = Instant::now();
    }
    assert_eq!(Instant::now() - started, ONE_HOUR);
}

#[test]
fn tc02_driver_has_no_advance_or_jump_method() {
    // The public surface of Driver is exactly run_for and sleep_ms: a source scan.
    let source = include_str!("../../legatus-testkit/src/virt/driver.rs");
    let public: Vec<&str> = source.lines().filter(|l| l.trim_start().starts_with("pub ")).collect();
    assert_eq!(public.len(), 3, "struct plus two methods: {public:?}");
    for banned in ["advance", "jump", "pause", "resume"] {
        assert!(!source.contains(&format!("fn {banned}")), "Driver must not have {banned}");
    }
}

#[test]
fn tc03_offsets_accept_whole_milliseconds_only() {
    for ok in [Duration::ZERO, Duration::from_millis(1), Duration::from_millis(7)] {
        assert_eq!(MsOffset::new(ok).map(MsOffset::duration), Ok(ok));
    }
    for bad in [Duration::from_nanos(50), Duration::from_nanos(999_999), Duration::from_micros(1500)] {
        assert_eq!(MsOffset::new(bad), Err(OffsetError::SubMillisecond), "{bad:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn tc04_wall_clock_follows_paused_time_and_jumps_leave_monotonic_alone() {
    let wall = SimWall::new(WallTime { unix_ms: 1_000_000 });
    let started = Instant::now();
    Driver::sleep_ms(2_000).await;
    assert_eq!(wall.now().unix_ms, 1_002_000);
    let before = Instant::now();
    wall.jump(-3_600_000);
    wall.jump(3_600_000 + 500);
    assert_eq!(Instant::now(), before, "a wall jump must not move the monotonic clock");
    assert_eq!(wall.now().unix_ms, 1_002_500);
    assert_eq!(Instant::now() - started, Duration::from_secs(2));
}

#[test]
fn tc05_same_instant_events_run_in_registration_order_over_20_runtimes() {
    let mut orders = Vec::new();
    for _ in 0..20 {
        let rt = virtual_runtime();
        let seen = Arc::new(Mutex::new(Vec::new()));
        rt.block_on(async {
            let mut handles = Vec::new();
            for id in 0..5u32 {
                let seen = seen.clone();
                handles.push(tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    seen.lock().unwrap().push(id);
                }));
            }
            for h in handles {
                h.await.unwrap();
            }
        });
        orders.push(seen.lock().unwrap().clone());
    }
    assert!(orders.iter().all(|o| *o == orders[0]), "orders differ: {orders:?}");
    assert_eq!(orders[0], vec![0, 1, 2, 3, 4]);
}

#[test]
fn tc14_keys_are_visited_sorted_over_20_runs() {
    let keys: Vec<u32> = (0..50).map(|i| (i * 37 + 11) % 101).collect();
    for run in 0..20 {
        let mut map = BTreeMap::new();
        let mut order = keys.clone();
        order.rotate_left(run % keys.len());
        for k in order {
            map.insert(k, ());
        }
        let visited = sorted_keys(&map);
        let mut sorted = visited.clone();
        sorted.sort_unstable();
        assert_eq!(visited, sorted);
    }
}
