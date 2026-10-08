//! Constants of the reply path (story 145).

/// Chunks read from the node and not yet handed to the harness: the chunk in hand. The relay is a
/// pull-through stream, so no adapter may add a queue.
pub const MAX_UNPULLED_CHUNKS: usize = 1;
/// The end marker of an OpenAI chat stream.
pub const CHAT_END_MARKER: &[u8] = b"data: [DONE]";
/// The end marker of an Anthropic Messages stream.
pub const MESSAGES_END_MARKER: &[u8] = b"message_stop";
/// The end marker of an OpenAI Responses stream.
pub const RESPONSES_END_MARKER: &[u8] = b"response.completed";

/// Flags the end of a reply sets for the event record (story 122 owns the field names).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndFlags {
    /// The reply ended complete but the end marker of its protocol was not seen.
    pub short_stream: bool,
    /// Status 200, no content text, no tool call and finish reason stop (PROPOSED).
    pub empty_completion: bool,
}
