//! Wire protocol names shared by every Legatus program.
use serde::{Deserialize, Serialize};

/// The protocol of a request or of a node endpoint. Wire form is snake_case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    OpenAiChat,
    AnthropicMessages,
    OpenAiResponses,
    Passthrough,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::OpenAiChat => "open_ai_chat",
            Protocol::AnthropicMessages => "anthropic_messages",
            Protocol::OpenAiResponses => "open_ai_responses",
            Protocol::Passthrough => "passthrough",
        }
    }
}
