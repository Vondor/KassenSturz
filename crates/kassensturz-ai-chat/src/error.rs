// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Why the chat couldn't answer a question (D-034).

use std::fmt;

use kassensturz_ai::AiError;
use kassensturz_ai_tools::ToolError;
use kassensturz_core::{ErrorReport, Report};

/// The chat stopped before the model answered. No variant carries a
/// question, an answer or anything from the book or the provider.
#[derive(Debug)]
pub enum ChatError {
    /// The provider couldn't be asked, or its answer couldn't be used.
    /// Its report is the provider's own (`ai.`).
    Ai(AiError),
    /// The user stopped the question. A request already on its way can't be
    /// called back; its answer is thrown away (D-193).
    Cancelled,
    /// The user saw a request before it went and didn't let it go.
    Stopped,
    /// The model kept calling functions without answering
    /// ([`crate::MAX_ROUNDS`] requests for one question).
    TooManyRounds,
    /// The overview for a model that can't call functions couldn't be read
    /// from the book.
    Book(ToolError),
}

impl From<AiError> for ChatError {
    fn from(error: AiError) -> Self {
        Self::Ai(error)
    }
}

impl fmt::Display for ChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ai(error) => error.fmt(f),
            other => f.write_str(other.report().code),
        }
    }
}

impl std::error::Error for ChatError {}

impl Report for ChatError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Ai(error) => error.report(),
            Self::Cancelled => ErrorReport::new("ai-chat.cancelled"),
            Self::Stopped => ErrorReport::new("ai-chat.stopped"),
            Self::TooManyRounds => ErrorReport::new("ai-chat.too-many-rounds"),
            Self::Book(error) => {
                ErrorReport::new("ai-chat.book-unreadable").caused_by(error.report())
            }
        }
    }
}

// The list of error codes lives in `kassensturz-store`'s source, so that the
// crates that must not depend on each other all check their codes the same
// way.
#[cfg(test)]
#[path = "../../kassensturz-store/src/error/list.rs"]
mod list;
#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::list::{check, documented, unused};
    use super::*;

    /// One error of each kind. A new error needs an example here.
    fn examples() -> Vec<ChatError> {
        vec![
            ChatError::Ai(AiError::NoConsent),
            ChatError::Cancelled,
            ChatError::Stopped,
            ChatError::TooManyRounds,
            ChatError::Book(ToolError::BadPeriod),
        ]
    }

    #[test]
    fn every_code_is_listed_with_its_parameters() {
        let rows = documented();
        let mut seen = HashSet::new();
        for error in examples() {
            check(&error.report(), &rows, &mut seen);
        }
        let unused = unused(&rows, &seen, &["ai-chat"]);
        assert!(
            unused.is_empty(),
            "listed in docs/ERROR_CODES.md but no error has them: {unused:?}"
        );
    }

    #[test]
    fn a_provider_error_keeps_its_code() {
        let error = ChatError::from(AiError::Timeout);
        assert_eq!(error.report().code, "ai.timeout");
        assert_eq!(error.to_string(), "ai.timeout");
        assert_eq!(ChatError::Cancelled.to_string(), "ai-chat.cancelled");
    }
}
