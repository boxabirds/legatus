//! A deterministic stream of conversation turns. Tokens are numbers, never text. `expected_d` is
//! the exact length of the prefix a turn shares with the turn before it, which is what a cache
//! can reuse. All draws come from the seed.
use crate::faults::rules::SeededRng;

/// Size of the token number space.
const VOCABULARY: u32 = 32_000;
/// The first turn is this many tokens long, plus a random amount.
const FIRST_TURN_BASE: usize = 200;
const FIRST_TURN_SPREAD: u64 = 100;
/// A normal turn adds this many tokens, plus a random amount.
const APPEND_BASE: usize = 20;
const APPEND_SPREAD: u64 = 60;
/// A tool call adds a call block and a larger result.
const TOOL_BASE: usize = 80;
const TOOL_SPREAD: u64 = 400;
/// A side request is short.
const SIDE_BASE: usize = 30;
const SIDE_SPREAD: u64 = 30;
/// A compaction keeps this many tokens of the start and adds a summary.
const COMPACTION_KEEP: usize = 40;
const SUMMARY_BASE: usize = 50;
const SUMMARY_SPREAD: u64 = 50;
/// A cancelled turn drops up to this many tokens of the end.
const CANCEL_DROP_MAX: u64 = 30;
/// The seed used, and printed, when none is given.
pub const DEFAULT_SEED: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenMode {
    Append,
    ToolCall,
    SideRequest,
    Compaction,
    Cancel,
    EditAt { position: u32 },
}

/// One turn: the tokens of the whole prompt, and the shared prefix with the turn before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub tokens: Vec<u32>,
    pub expected_d: u32,
}

pub struct ConversationGen {
    rng: SeededRng,
    mode: GenMode,
    seed: u64,
}

impl ConversationGen {
    pub fn new(seed: u64, mode: GenMode) -> ConversationGen {
        ConversationGen { rng: SeededRng::new(seed), mode, seed }
    }

    /// With no seed the default is used and printed by `announce`, so a failure can be repeated.
    pub fn with_optional_seed(seed: Option<u64>, mode: GenMode, announce: &mut dyn FnMut(String)) -> ConversationGen {
        let seed = seed.unwrap_or_else(|| {
            announce(format!("conversation seed: {DEFAULT_SEED}"));
            DEFAULT_SEED
        });
        ConversationGen::new(seed, mode)
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    fn tokens(&mut self, base: usize, spread: u64) -> Vec<u32> {
        let count = base + usize::try_from(self.rng.next_u64() % (spread + 1)).unwrap_or(0);
        (0..count).map(|_| u32::try_from(self.rng.next_u64() % u64::from(VOCABULARY)).unwrap_or(0)).collect()
    }

    /// The next turn in the mode of the generator. With no previous turn it is a first turn.
    pub fn next_turn(&mut self, prev: Option<&Turn>) -> Turn {
        let Some(prev) = prev else {
            return Turn { tokens: self.tokens(FIRST_TURN_BASE, FIRST_TURN_SPREAD), expected_d: 0 };
        };
        let previous = &prev.tokens;
        match self.mode {
            GenMode::Append => {
                let mut tokens = previous.clone();
                tokens.extend(self.tokens(APPEND_BASE, APPEND_SPREAD));
                Turn { expected_d: len_u32(previous.len()), tokens }
            }
            GenMode::ToolCall => {
                let mut tokens = previous.clone();
                tokens.extend(self.tokens(TOOL_BASE, TOOL_SPREAD));
                Turn { expected_d: len_u32(previous.len()), tokens }
            }
            GenMode::SideRequest => Turn { tokens: self.tokens(SIDE_BASE, SIDE_SPREAD), expected_d: 0 },
            GenMode::Compaction => {
                let keep = COMPACTION_KEEP.min(previous.len());
                let mut tokens = previous[..keep].to_vec();
                tokens.extend(self.tokens(SUMMARY_BASE, SUMMARY_SPREAD));
                Turn { expected_d: len_u32(keep), tokens }
            }
            GenMode::Cancel => {
                let drop = usize::try_from(self.rng.next_u64() % (CANCEL_DROP_MAX + 1)).unwrap_or(0).min(previous.len().saturating_sub(1));
                let keep = previous.len() - drop;
                let mut tokens = previous[..keep].to_vec();
                tokens.extend(self.tokens(APPEND_BASE, APPEND_SPREAD));
                Turn { expected_d: len_u32(keep), tokens }
            }
            GenMode::EditAt { position } => {
                let at = usize::try_from(position).unwrap_or(usize::MAX).min(previous.len());
                let mut tokens = previous[..at].to_vec();
                // A different token at the edit point, then new text, so the shared prefix is exact.
                let mut replacement = self.tokens(APPEND_BASE, APPEND_SPREAD);
                if let (Some(old), Some(new)) = (previous.get(at), replacement.first_mut()) {
                    if old == new {
                        *new = (*new + 1) % VOCABULARY;
                    }
                }
                tokens.extend(replacement);
                Turn { expected_d: len_u32(at), tokens }
            }
        }
    }
}

fn len_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
