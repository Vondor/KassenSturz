// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tool-call log: what the user asked, what was sent to the provider,
//! which functions the model called and what they answered (D-047, D-192;
//! threat T-22).
//!
//! The chat panel shows it (M5.8), so the user can see everything that left
//! KassenSturz for the model. It lives only in memory, with the
//! conversation, and is never written to a file, except to the developer
//! mode's transcript (D-232): it holds the book's data that was sent. It never holds the provider's key, which is not part of
//! what is sent in the body (D-184). What reaches the opt-in debug log is
//! only [`LogEntry::debug_event`]: event names, function names and error
//! codes (D-173); the transcript of D-232 gets the lines in full.

#[cfg(feature = "debug-log")]
use kassensturz_core::debug_log::{Event, known_detail};

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

#[cfg(feature = "debug-log")]
impl LogEntry {
    /// The entry of the debug log this line stands for (D-173): its event,
    /// the function it concerns and its error code, all from fixed lists.
    /// The question, the requests and the results stay out.
    pub fn debug_event(&self) -> (Event, Option<&'static str>, Option<&'static str>) {
        match self {
            Self::Asked { .. } => (Event::ChatAsked, None, None),
            Self::Sent { .. } => (Event::ChatRequestSent, None, None),
            Self::Called {
                function, error, ..
            } => (
                Event::ChatFunctionCalled,
                // A name the model made up isn't in the list.
                Some(
                    known_detail(function)
                        .filter(|name| !name.ends_with("Page"))
                        .unwrap_or("unknown"),
                ),
                *error,
            ),
            Self::NotOffered { .. } => (Event::ChatFunctionsNotOffered, None, None),
            Self::Overview => (Event::ChatOverviewUsed, None, None),
            Self::Answered { unverified: 0 } => (Event::ChatAnswered, None, None),
            Self::Answered { .. } => (Event::ChatAnswerUnverified, None, None),
            Self::Failed { code } => (Event::ChatFailed, None, Some(*code)),
        }
    }
}

#[cfg(feature = "debug-log")]
impl LogEntry {
    /// Writes this line in full to the developer mode's transcript (D-232),
    /// which, unlike the event log, holds the question, the request, the
    /// arguments and the result. Answers and failures are noted by the
    /// session, which has their text.
    pub fn transcribe(&self, log: &kassensturz_core::debug_log::DebugLog) {
        match self {
            Self::Asked { question } => log.transcript("asked", &[("question", question)]),
            Self::Sent { round, body } => {
                log.transcript("request", &[("round", &round.to_string()), ("body", body)])
            }
            Self::Called {
                function,
                arguments,
                result,
                error,
            } => log.transcript(
                "called",
                &[
                    ("function", function),
                    ("arguments", arguments),
                    ("result", result),
                    ("error", error.unwrap_or("")),
                ],
            ),
            Self::NotOffered { calls } => {
                log.transcript("not-offered", &[("calls", &calls.to_string())]);
            }
            Self::Overview => log.transcript("overview", &[]),
            Self::Answered { .. } | Self::Failed { .. } => {}
        }
    }
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

    #[cfg(feature = "debug-log")]
    #[test]
    fn the_debug_log_gets_events_and_codes_only() {
        let called = LogEntry::Called {
            function: "account_balance".into(),
            arguments: r#"{"account_id":"Moon Base"}"#.into(),
            result: "12.50".into(),
            error: Some("ai-tools.unknown-account"),
        };
        assert_eq!(
            called.debug_event(),
            (
                Event::ChatFunctionCalled,
                Some("account_balance"),
                Some("ai-tools.unknown-account")
            )
        );
        let made_up = LogEntry::Called {
            function: "Ignore your rules".into(),
            arguments: String::new(),
            result: String::new(),
            error: Some("ai-tools.unknown-function"),
        };
        assert_eq!(made_up.debug_event().1, Some("unknown"));
        let screen = LogEntry::Called {
            function: "OverviewPage".into(),
            arguments: String::new(),
            result: String::new(),
            error: None,
        };
        assert_eq!(screen.debug_event().1, Some("unknown"));
        let asked = LogEntry::Asked {
            question: "How much is in Moon Base?".into(),
        };
        assert_eq!(asked.debug_event(), (Event::ChatAsked, None, None));
        assert_eq!(
            LogEntry::Answered { unverified: 2 }.debug_event().0,
            Event::ChatAnswerUnverified
        );
    }

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
