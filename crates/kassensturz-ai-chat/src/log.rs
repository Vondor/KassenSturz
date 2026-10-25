// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tool-call log: what the user asked, what was sent to the provider,
//! which functions the model called and what they answered (D-047, D-192;
//! threat T-22).
//!
//! The chat panel shows it (M5.8), so the user can see everything that left
//! KassenSturz for the model. It lives only in memory, with the
//! conversation, and is never written to a file: it holds the book's data
//! that was sent. It never holds the provider's key, which is not part of
//! what is sent in the body (D-184).

/// The most characters of a function call's arguments the log keeps. The
/// arguments are the model's text and untrusted (X-8).
pub const MAX_ARGUMENT_CHARS: usize = 1_000;

/// One line of the tool-call log, in the order it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogEntry {
    /// The user asked a question.
    Asked {
        /// The question, as the user wrote it.
        question: String,
    },
    /// A request went to the provider.
    Sent {
        /// Which request for this question it was, from 1.
        round: usize,
        /// The exact body that was sent ([`kassensturz_ai::PreparedRequest::preview`]),
        /// after the user's redactions.
        body: String,
    },
    /// The model called a read function, and KassenSturz answered.
    Called {
        /// The function, always one of the fixed list (D-170).
        function: String,
        /// The arguments as the model wrote them, cut to
        /// [`MAX_ARGUMENT_CHARS`]. Untrusted text, shown as plain text.
        arguments: String,
        /// What the model got back: the result or the error, as JSON.
        result: String,
        /// The error code when the function couldn't answer.
        error: Option<&'static str>,
    },
    /// The model asked for functions that aren't offered. They were not
    /// run, and the model was told so (threat T-31).
    NotOffered {
        /// How many calls.
        calls: usize,
    },
    /// The provider refused the functions, so the chat goes on with the
    /// small overview instead (D-052, D-190).
    Overview,
    /// The model answered.
    Answered {
        /// How many numbers in the answer no function produced (D-188).
        unverified: usize,
    },
    /// The question ended without an answer.
    Failed {
        /// Why, as an error code.
        code: &'static str,
    },
}

/// `text` cut to `max` characters, with `…` when it was longer.
pub(crate) fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_arguments_are_cut() {
        assert_eq!(cut("{}", 3), "{}");
        assert_eq!(cut("äöüß", 2), "äö…");
        assert_eq!(
            cut(&"x".repeat(5000), MAX_ARGUMENT_CHARS).chars().count(),
            MAX_ARGUMENT_CHARS + 1
        );
    }
}
