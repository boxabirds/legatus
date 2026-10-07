//! Fault rule engine: matching and limit accounting, plus the seeded generator.
use crate::stubs::scenario::{FaultAction, FaultRule, Limit};
use crate::virt::MsOffset;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

/// Deterministic generator (xorshift64*). Randomness in the kit comes only from here.
#[derive(Debug, Clone)]
pub struct SeededRng(u64);

const XORSHIFT_MULTIPLIER: u64 = 0x2545_F491_4F6C_DD1D;
const ZERO_SEED_REPLACEMENT: u64 = 0x9E37_79B9_7F4A_7C15;

impl SeededRng {
    pub fn new(seed: u64) -> SeededRng {
        SeededRng(if seed == 0 { ZERO_SEED_REPLACEMENT } else { seed })
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(XORSHIFT_MULTIPLIER)
    }
    /// A whole-millisecond offset in `0..=max_ms`.
    pub fn arrival_offset(&mut self, max_ms: u64) -> MsOffset {
        let ms = self.next_u64() % (max_ms + 1);
        MsOffset::new(Duration::from_millis(ms)).expect("whole milliseconds")
    }
}

struct RuleState {
    matched: u32,
    fired: u32,
}

/// Decides which rule fires for a request. Time is measured from `started`.
pub struct RuleEngine {
    rules: Vec<FaultRule>,
    state: Mutex<Vec<RuleState>>,
    started: Instant,
}

impl RuleEngine {
    pub fn new(rules: Vec<FaultRule>) -> RuleEngine {
        let state = rules.iter().map(|_| RuleState { matched: 0, fired: 0 }).collect();
        RuleEngine { rules, state: Mutex::new(state), started: Instant::now() }
    }

    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The action of the first rule that matches `path` and whose limit is open, if any.
    pub fn decide(&self, path: &str) -> Option<FaultAction> {
        let now_ms = self.elapsed_ms();
        let mut state = self.state.lock().ok()?;
        for (i, rule) in self.rules.iter().enumerate() {
            if matches!(rule.action, FaultAction::SinkFault(_) | FaultAction::ClockJump { .. }) {
                continue; // applied through the sink and the clock, not to requests
            }
            if let Some(p) = &rule.when.path {
                if p != path {
                    continue;
                }
            }
            state[i].matched += 1;
            if let Some(nth) = rule.when.nth {
                if state[i].matched != nth {
                    continue;
                }
            }
            let once_spent = matches!(rule.action, FaultAction::Reset { once: true }) && state[i].fired >= 1;
            let open = !once_spent && match rule.limit {
                Limit::Always => true,
                Limit::Count(n) => state[i].fired < n,
                Limit::Window { from_ms, to_ms } => (from_ms..to_ms).contains(&now_ms),
            };
            if open {
                state[i].fired += 1;
                return Some(rule.action.clone());
            }
        }
        None
    }
}
