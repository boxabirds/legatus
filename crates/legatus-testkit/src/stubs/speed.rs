//! Speed model of the stub engines (story 162). Shapes only: the tables come from one M2 (spike s3)
//! and from second-hand figures (research r4).

/// Prefill and decode tables of one engine.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeedModel {
    /// (context tokens, prefill tokens per second R).
    pub prefill: Vec<(u32, f64)>,
    /// (running requests N, aggregate decode tokens per second).
    pub decode: Vec<(u32, f64)>,
    pub load_time_ms: u64,
}

const MS_PER_S: f64 = 1000.0;
/// Prefill rate measured in spike s3 (250 to 270 tok/s).
const SPIKE_S3_PREFILL_TOK_S: f64 = 260.0;
const DEFAULT_LOAD_TIME_MS: u64 = 2000;

fn interpolate(table: &[(u32, f64)], x: u32) -> f64 {
    let Some(&(first_x, first_y)) = table.first() else { return 0.0 };
    if x <= first_x {
        return first_y;
    }
    for pair in table.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if x <= x1 {
            let t = f64::from(x - x0) / f64::from(x1 - x0);
            return y0 + (y1 - y0) * t;
        }
    }
    table.last().map(|&(_, y)| y).unwrap_or(0.0)
}

impl SpeedModel {
    /// First token time: queue wait + recompute / (R / N), R from the prefill table at `ctx`.
    pub fn first_token_ms(&self, recompute: u32, ctx: u32, prefill_running: u32, queue_wait_ms: u64) -> u64 {
        if recompute == 0 {
            return queue_wait_ms;
        }
        let r = interpolate(&self.prefill, ctx);
        if r <= 0.0 {
            return queue_wait_ms;
        }
        let n = f64::from(prefill_running.max(1));
        queue_wait_ms + (f64::from(recompute) / (r / n) * MS_PER_S).round() as u64
    }

    /// Per-request decode rate: the aggregate for `running` requests divided by `running`.
    /// Clamped at the largest table point (PROPOSED).
    pub fn decode_rate(&self, running: u32) -> f64 {
        let n = running.max(1);
        interpolate(&self.decode, n) / f64::from(n)
    }

    /// Whole milliseconds between two output tokens at the current load.
    pub fn token_interval_ms(&self, running: u32) -> u64 {
        let rate = self.decode_rate(running);
        if rate <= 0.0 { 0 } else { (MS_PER_S / rate).round() as u64 }
    }

    fn with(decode: [f64; 4]) -> SpeedModel {
        SpeedModel {
            prefill: vec![(1, SPIKE_S3_PREFILL_TOK_S)],
            decode: [1u32, 2, 4, 8].into_iter().zip(decode).collect(),
            load_time_ms: DEFAULT_LOAD_TIME_MS,
        }
    }
    pub fn llama_4_slots() -> SpeedModel { Self::with([28.0, 34.0, 46.0, 49.0]) }
    pub fn ollama_4_parallel() -> SpeedModel { Self::with([27.0, 33.0, 43.0, 49.0]) }
    pub fn llama_1_slot() -> SpeedModel { Self::with([34.0, 25.0, 27.0, 25.0]) }
    pub fn ollama_default() -> SpeedModel { Self::with([49.0, 27.0, 24.0, 25.0]) }
    pub fn mlx() -> SpeedModel { Self::with([55.0, 58.0, 53.0, 51.0]) }
    /// Second-hand prefill figures: Strix Halo llama.cpp 351 tok/s at 2k and 172 at 120k tokens.
    pub fn strix_halo() -> SpeedModel {
        SpeedModel { prefill: vec![(2_000, 351.0), (120_000, 172.0)], ..Self::llama_4_slots() }
    }
    pub fn by_name(name: &str) -> Option<SpeedModel> {
        Some(match name {
            "llama_4_slots" => Self::llama_4_slots(),
            "ollama_4_parallel" => Self::ollama_4_parallel(),
            "llama_1_slot" => Self::llama_1_slot(),
            "ollama_default" => Self::ollama_default(),
            "mlx" => Self::mlx(),
            "strix_halo" => Self::strix_halo(),
            _ => return None,
        })
    }
}
