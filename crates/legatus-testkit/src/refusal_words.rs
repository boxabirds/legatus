//! The forbidden-word checker of the refusal catalogue, for every refusal test in every epic
//! (story 160). The lists and the matching live in the proxy library; this module re-exports
//! them and adds an assertion helper, so tests never keep their own copy of the words.
pub use legatus_proxy::protocol::errors::{forbidden_word_in, WordList};

/// Panic with the body and the word when a body that must stop (or a 400 or 409 body) holds a word
/// of the `Stop` list.
pub fn assert_no_stop_word(body: &str) {
    if let Some(word) = forbidden_word_in(WordList::Stop, body) {
        panic!("the refusal body holds the word {word:?}: {body}");
    }
}

/// Panic when a hold-limit or queue-full body holds a word of the `HoldBody` list.
pub fn assert_no_hold_word(body: &str) {
    if let Some(word) = forbidden_word_in(WordList::HoldBody, body) {
        panic!("the hold body holds the word {word:?}: {body}");
    }
}
