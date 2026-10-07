//! The call site of the alias lookup. Story 121 adds the rest of the routing.
use crate::config::alias::{AliasSpec, AliasTable, ResolveError};

/// Error code of an unknown model name (contract C23, story 160 owns the body).
pub const MODEL_NOT_FOUND_CODE: &str = "model_not_found";

/// Why a request is refused before any node is contacted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    ModelNotFound,
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Refusal::ModelNotFound => MODEL_NOT_FOUND_CODE,
        }
    }
}

/// Pick the alias for the model value of a request. No node is contacted on a refusal, and the
/// value is neither logged nor returned.
pub fn route_by_model<'a>(table: &'a AliasTable, model: &str) -> Result<&'a AliasSpec, Refusal> {
    table.resolve(model).map_err(|ResolveError::UnknownModel| Refusal::ModelNotFound)
}
