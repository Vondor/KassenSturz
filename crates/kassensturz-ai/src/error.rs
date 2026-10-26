// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What can go wrong between KassenSturz and an AI provider: codes without
//! values (D-034).

use std::fmt;

use kassensturz_core::{ErrorParam, ErrorReport, Report};

/// The provider couldn't be asked, or its answer couldn't be used. No
/// variant carries a key, a question, an answer or the provider's own text:
/// a report, a log line or a panic message that shows one shows nothing
/// worth protecting (threats T-25, T-31). The front end turns the code into
/// a message (`docs/ERROR_CODES.md`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AiError {
    /// AI is switched off on this computer (D-013, D-177). Nothing was sent.
    Off,
    /// No provider is set up: the address or the model is missing.
    NoProvider,
    /// The provider's address isn't a web address KassenSturz can use: not
    /// `http://` or `https://`, no server name, or it carries a login, a
    /// query or a `#` part.
    BadAddress,
    /// The address uses plain `http://` for another computer. Only this
    /// computer may be reached without encryption (D-175, Q-125).
    PlainHttp,
    /// The provider is on another computer and the user hasn't agreed to
    /// what is sent to it: never, for another address, or for an older
    /// statement of what is sent (D-035, D-168). Nothing was sent.
    NoConsent,
    /// The provider's settings changed after the request was prepared, so
    /// the preview the user saw is not for the provider now set up. Nothing
    /// was sent.
    SettingsChanged,
    /// The request is larger than KassenSturz sends at once.
    RequestTooLarge,
    /// The provider needs a key and none was given, remembered or saved.
    KeyNeeded,
    /// The provider refused the key (HTTP 401 or 403).
    KeyRejected,
    /// The system keychain couldn't save, read or remove the key.
    Keychain,
    /// KeePassXC couldn't save, read or remove the key (D-164).
    KeePassXc,
    /// The provider's server couldn't be reached: no such name, no
    /// connection, or the connection broke.
    Unreachable,
    /// The server's certificate didn't pass the system's checks, so nothing
    /// was sent to it (threat T-30).
    UntrustedServer,
    /// The provider took longer than KassenSturz waits.
    Timeout,
    /// The provider answered with a redirect, which KassenSturz doesn't
    /// follow (threat T-31).
    Redirect,
    /// The provider says the address or the model doesn't exist (HTTP 404).
    NotFound,
    /// The address has no path, the provider's answer to it is an error
    /// (some providers, e.g. LM Studio, send one with HTTP 200), and the
    /// same address with `/v1` is the provider's interface (D-233).
    MissingV1,
    /// The provider asks to wait before the next request (HTTP 429).
    RateLimited,
    /// The provider answered with another error.
    ProviderFailed {
        /// The HTTP status of the answer.
        status: u16,
    },
    /// The answer is larger than KassenSturz reads (threat T-31).
    ReplyTooLarge,
    /// The answer isn't what an OpenAI-compatible provider sends: not JSON,
    /// nested too deeply, or without the expected parts (threat T-31).
    BadReply,
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderFailed { status } => write!(f, "the provider failed (HTTP {status})"),
            other => f.write_str(other.report().code),
        }
    }
}

impl std::error::Error for AiError {}

impl Report for AiError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::Off => ErrorReport::new("ai.off"),
            Self::NoProvider => ErrorReport::new("ai.no-provider"),
            Self::BadAddress => ErrorReport::new("ai.bad-address"),
            Self::PlainHttp => ErrorReport::new("ai.plain-http"),
            Self::NoConsent => ErrorReport::new("ai.no-consent"),
            Self::SettingsChanged => ErrorReport::new("ai.settings-changed"),
            Self::RequestTooLarge => ErrorReport::new("ai.request-too-large"),
            Self::KeyNeeded => ErrorReport::new("ai.key-needed"),
            Self::KeyRejected => ErrorReport::new("ai.key-rejected"),
            Self::Keychain => ErrorReport::new("ai.keychain"),
            Self::KeePassXc => ErrorReport::new("ai.keepassxc"),
            Self::Unreachable => ErrorReport::new("ai.unreachable"),
            Self::UntrustedServer => ErrorReport::new("ai.untrusted-server"),
            Self::Timeout => ErrorReport::new("ai.timeout"),
            Self::Redirect => ErrorReport::new("ai.redirect"),
            Self::NotFound => ErrorReport::new("ai.not-found"),
            Self::MissingV1 => ErrorReport::new("ai.missing-v1"),
            Self::RateLimited => ErrorReport::new("ai.rate-limited"),
            Self::ProviderFailed { status } => ErrorReport::new("ai.provider-failed")
                .with("status", ErrorParam::Number(i64::from(*status))),
            Self::ReplyTooLarge => ErrorReport::new("ai.reply-too-large"),
            Self::BadReply => ErrorReport::new("ai.bad-reply"),
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
mod tests;
