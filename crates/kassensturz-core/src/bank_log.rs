// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bank log of the developer mode for real bank connections (D-603,
//! amends D-227; threats T-06, T-33).
//!
//! Someone who tries a real bank needs to see how far a connection got and
//! why it stopped, and must be able to show that to others without
//! showing anything of their finances. The log is written only while the
//! program was started from a terminal with `KASSENSTURZ_BANK_LOG=1`
//! ([`ENV`]). The app has no switch for it, a banner shows while it runs,
//! and it is documented in the repository only. It is separate from the
//! debug log (D-227), so one can be shared without the other.
//!
//! **Redacted by construction.** An entry holds the time, a [`Step`] (an
//! enum), an error code (a `&'static str` of the shape `area.word-word`,
//! checked again before writing), a small count (accounts found,
//! transactions fetched, the TAN method's number is *not* one) and the
//! milliseconds the step took. Nothing here takes a `String`, so a login
//! name, IBAN, bank code, amount, PIN, TAN, registration number or text from
//! the bank has no way in. Not even the bank's name is written.

use std::sync::OnceLock;
use std::time::Instant;

use jiff::Timestamp;

use crate::debug_log::{append, default_folder};
use crate::is_error_code;

/// The environment variable that starts the bank log: `1` is on, `2` is on
/// with the bank's own reply texts, redacted (D-619); anything else is off.
pub const ENV: &str = "KASSENSTURZ_BANK_LOG";

/// The log's folder, beside the debug log's.
const FOLDER_NAME: &str = "bank-log";

/// The log's file, and the one before it.
const FILES: (&str, &str) = ("bank-log.jsonl", "bank-log-older.jsonl");

/// How large the file gets before it becomes the older one.
pub const MAX_FILE_BYTES: u64 = 128 * 1024;

macro_rules! steps {
    ($($(#[$doc:meta])* $variant:ident = $name:literal,)*) => {
        /// How far a bank connection got. The names are stable.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Step {
            $($(#[$doc])* $variant,)*
        }

        impl Step {
            /// Every step, in the order above.
            pub const ALL: &[Step] = &[$(Step::$variant,)*];

            /// Its name in the log, e.g. `bank.fetch-started`.
            pub fn name(self) -> &'static str {
                match self {
                    $(Step::$variant => $name,)*
                }
            }
        }
    };
}

steps! {
    /// Setting up a login began.
    SetupStarted = "bank.setup-started",
    /// A login was set up; the count is the accounts the bank listed.
    SetupDone = "bank.setup-done",
    /// Setting up failed; the code says why.
    SetupFailed = "bank.setup-failed",
    /// A fetch began; the count is the accounts it is for.
    FetchStarted = "bank.fetch-started",
    /// One account's fetch ended well; the count is the booked entries.
    FetchDone = "bank.fetch-done",
    /// One account's fetch failed; the code says why.
    FetchFailed = "bank.fetch-failed",
    /// One account was left out of a fetch; the code says why.
    FetchSkipped = "bank.fetch-skipped",
    /// The whole fetch could not start; the code says why.
    FetchRefused = "bank.fetch-refused",
    /// The registration number was asked for (never the number itself).
    RegistrationAsked = "bank.registration-asked",
    /// The user answered the registration question; the count is 1 for a
    /// number given, 0 for cancel.
    RegistrationAnswered = "bank.registration-answered",
    /// The bank asked for the PIN.
    PinAsked = "bank.pin-asked",
    /// The user answered the PIN question; the count is 1 for a PIN given,
    /// 0 for cancel.
    PinAnswered = "bank.pin-answered",
    /// The bank asked for a TAN.
    TanAsked = "bank.tan-asked",
    /// The user answered the TAN question; the count is 1 for a TAN given,
    /// 0 for cancel.
    TanAnswered = "bank.tan-answered",
    /// The bank asked a question to answer in its app or in the window.
    QuestionAsked = "bank.question-asked",
    /// The user answered the bank's question; the count is 1 for yes or
    /// approved, 0 for no or cancel.
    QuestionAnswered = "bank.question-answered",
    /// A check that AqBanking wrote no message log (D-261), after each bank
    /// call; the count is 1 when it left none, 0 when it left some (which
    /// were deleted) or the folder could not be read.
    MessageLogCheck = "bank.message-log-check",
    /// A return code the bank sent back (FinTS, e.g. 9931 for a wrong
    /// login or PIN); the count is the code. Only codes from the public
    /// list of the FinTS specification are taken, never any other text or
    /// number from the bank (D-607).
    BankReply = "bank.bank-reply",
    /// A fetch looked for the bank account a book account is linked to
    /// (D-607). The count is how many logins it looked through.
    LookupLogins = "bank.lookup-logins",
    /// The same lookup: how many bank accounts those logins list.
    LookupAccounts = "bank.lookup-accounts",
    /// The same lookup: how many of the listed accounts have an IBAN.
    LookupIbans = "bank.lookup-ibans",
    /// The same lookup: what the link names, as a sum: 1 for an IBAN, 2 for
    /// a bank code and number, 4 when some listed account has the link's
    /// bank code. Never the values themselves.
    LookupLink = "bank.lookup-link",
    /// A lookup found no login: how many login settings files AqBanking's
    /// folder holds (M6-B181).
    UserFiles = "bank.user-files",
    /// The same: how many of those files give a login ID.
    UserFilesWithId = "bank.user-files-with-id",
    /// The same: how many of those say they are a PIN/TAN login.
    UserFilesPinTan = "bank.user-files-pin-tan",
    /// Level 2 only (D-619): the bank's reply text for a FinTS code; the
    /// count is the code, the text is redacted by [`redact_notice`].
    BankNotice = "bank.bank-notice",
}

/// How many characters of a bank's reply text the log keeps.
pub const NOTICE_MAX_CHARS: usize = 160;

/// The bank's reply text as level 2 of the log may hold it (D-619): the
/// words only. A word with a digit in it (an account number, an IBAN, a
/// date, an amount), a word with an `@`, a dot between two letters (a
/// domain), exactly 8 or 11 capitals (the shape of a BIC) and any word of
/// 25 characters or more is replaced by `#`; of the rest only letters (also umlauts) and
/// `.,:;()-/'` stay, other characters become spaces; the text is cut to
/// [`NOTICE_MAX_CHARS`]. A name the bank puts into its text in words is the
/// one thing this cannot see, which is why level 2 is for the person's
/// own tries and the log is looked at before it is shown.
pub fn redact_notice(text: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        if hides_something(word) {
            words.push("#".to_owned());
            continue;
        }
        let kept: String = word
            .chars()
            .map(|c| {
                if c.is_alphabetic() || ".,:;()-/'".contains(c) {
                    c
                } else {
                    ' '
                }
            })
            .collect();
        let kept = kept.trim();
        if !kept.is_empty() {
            words.push(kept.to_owned());
        }
    }
    let joined = words.join(" ");
    joined.chars().take(NOTICE_MAX_CHARS).collect()
}

/// Whether a word of a bank's text could be an identifier: a digit or an
/// `@` in it, 25 characters or more, a dot between two letters (a domain or
/// address), or exactly 8 or 11 capitals (the shape of a BIC).
fn hides_something(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    if chars.iter().any(|c| c.is_ascii_digit() || *c == '@') || chars.len() >= 25 {
        return true;
    }
    if chars
        .windows(3)
        .any(|w| w[1] == '.' && w[0].is_alphabetic() && w[2].is_alphabetic())
    {
        return true;
    }
    let core: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    matches!(core.len(), 8 | 11) && core.iter().all(|c| c.is_uppercase())
}

/// One line of level 2 holding a bank reply: the time, the step, the FinTS
/// code as the count and the redacted text.
pub fn notice_line(at: Timestamp, code: u16, text: &str) -> String {
    let text = redact_notice(text);
    // Letters and the punctuation above only: nothing to escape in JSON.
    format!(
        "{{\"at\":\"{}\",\"step\":\"{}\",\"n\":{code},\"text\":\"{text}\"}}\n",
        at.strftime("%Y-%m-%dT%H:%M:%S%.3fZ"),
        Step::BankNotice.name()
    )
}

/// One line of the log, without the line break. `code` that isn't an error
/// code is replaced by `bank.unknown-code`, so a stray text can't get in.
pub fn line(
    at: Timestamp,
    step: Step,
    code: Option<&'static str>,
    count: Option<u32>,
    ms: Option<u64>,
) -> String {
    let mut line = format!(
        "{{\"at\":\"{}\",\"step\":\"{}\"",
        at.strftime("%Y-%m-%dT%H:%M:%S%.3fZ"),
        step.name()
    );
    if let Some(code) = code {
        let code = if is_error_code(code) {
            code
        } else {
            "bank.unknown-code"
        };
        line.push_str(&format!(",\"code\":\"{code}\""));
    }
    if let Some(count) = count {
        line.push_str(&format!(",\"n\":{count}"));
    }
    if let Some(ms) = ms {
        line.push_str(&format!(",\"ms\":{ms}"));
    }
    line.push_str("}\n");
    line
}

/// The code that stands for why *Update all accounts* left an account out
/// (`cancelled`, `bank-failed`, `earlier-failure`, `stopped`), so the log
/// holds a code of the shape of every other and nothing else. Any other
/// reason is `bank.unknown-code`.
pub fn skip_code(reason: &str) -> &'static str {
    match reason {
        "cancelled" => "bank.cancelled",
        "bank-failed" => "bank.bank-failed",
        "earlier-failure" => "bank.earlier-failure",
        "stopped" => "bank.stopped",
        _ => "bank.unknown-code",
    }
}

/// Whether the bank log runs: the program was started with
/// `KASSENSTURZ_BANK_LOG=1` and knows where the log goes. Read once.
pub fn is_on() -> bool {
    level() > 0
}

/// Whether level 2 runs (D-619): the bank's reply texts, redacted.
pub fn is_detailed() -> bool {
    level() > 1
}

/// The level the program was started with (`1`, `2`), `0` for off. Read once.
fn level() -> u8 {
    static LEVEL: OnceLock<u8> = OnceLock::new();
    *LEVEL.get_or_init(|| {
        let level = match std::env::var_os(ENV) {
            Some(value) if value == "1" => 1,
            Some(value) if value == "2" => 2,
            _ => 0,
        };
        if level > 0 && folder().is_some() {
            level
        } else {
            0
        }
    })
}

/// Notes the bank's reply `text` for the FinTS `code` (level 2 only,
/// D-619), redacted. Does nothing below level 2.
pub fn note_notice(code: u16, text: &str) {
    if !is_detailed() {
        return;
    }
    if let Some(folder) = folder() {
        let _ = append(
            &folder,
            FILES,
            MAX_FILE_BYTES,
            &notice_line(Timestamp::now(), code, text),
        );
    }
}

/// The log's folder, where this build knows it.
pub fn folder() -> Option<std::path::PathBuf> {
    default_folder().and_then(|debug| debug.parent().map(|parent| parent.join(FOLDER_NAME)))
}

/// Notes `step` now. Does nothing when the log is off; a failure to write
/// is ignored, a log must never break a bank call.
pub fn note(step: Step, code: Option<&'static str>, count: Option<u32>) {
    note_after(step, code, count, None);
}

/// Like [`note`], with the time since `since`, in milliseconds.
pub fn note_after(
    step: Step,
    code: Option<&'static str>,
    count: Option<u32>,
    since: Option<Instant>,
) {
    if !is_on() {
        return;
    }
    if let Some(folder) = folder() {
        let ms = since.map(|since| u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX));
        let text = line(Timestamp::now(), step, code, count, ms);
        let _ = append(&folder, FILES, MAX_FILE_BYTES, &text);
    }
}

/// Deletes the log's files. Returns whether that worked.
pub fn clear() -> bool {
    folder().is_some_and(|folder| {
        [FILES.0, FILES.1]
            .iter()
            .all(|name| match std::fs::remove_file(folder.join(name)) {
                Ok(()) => true,
                Err(error) => error.kind() == std::io::ErrorKind::NotFound,
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_holds_only_the_fixed_parts() {
        let at: Timestamp = "2026-11-06T16:00:00Z".parse().unwrap();
        let text = line(
            at,
            Step::FetchFailed,
            Some("bank.pin-rejected"),
            Some(3),
            Some(120),
        );
        assert_eq!(
            text,
            "{\"at\":\"2026-11-06T16:00:00.000Z\",\"step\":\"bank.fetch-failed\",\"code\":\"bank.pin-rejected\",\"n\":3,\"ms\":120}\n"
        );
    }

    #[test]
    fn text_that_is_not_a_code_never_gets_in() {
        let at: Timestamp = "2026-11-06T16:00:00Z".parse().unwrap();
        for stray in [
            "DE89 3704 0044 0532 0130 00",
            "12345",
            "my pin",
            "bank.pin rejected\"}",
        ] {
            let text = line(at, Step::PinAsked, Some(stray), None, None);
            assert!(text.contains("bank.unknown-code"), "{text}");
            assert!(!text.contains(stray), "{text}");
        }
    }

    #[test]
    fn every_skip_reason_has_a_code_of_its_own() {
        let at: Timestamp = "2026-11-06T16:00:00Z".parse().unwrap();
        for reason in ["cancelled", "bank-failed", "earlier-failure", "stopped"] {
            let code = skip_code(reason);
            assert_ne!(code, "bank.unknown-code", "{reason}");
            assert!(is_error_code(code), "{code}");
            assert!(line(at, Step::FetchSkipped, Some(code), None, None).contains(code));
        }
        assert_eq!(skip_code("anything else"), "bank.unknown-code");
    }

    #[test]
    fn a_bank_notice_holds_words_only() {
        for (text, expected) in [
            (
                "Nachrichtenaufbau fehlerhaft",
                "Nachrichtenaufbau fehlerhaft",
            ),
            ("Konto 1234567890 gesperrt", "Konto # gesperrt"),
            ("IBAN DE89370400440532013000 ungültig", "IBAN # ungültig"),
            ("Mail a@b.de gesendet", "Mail # gesendet"),
            (
                "Betrag 12,50 EUR über Limit 9000",
                "Betrag # EUR über Limit #",
            ),
            ("Segment \"HKTAN\" {fehlt}\n", "Segment HKTAN fehlt"),
            (
                "Auftrag abgelehnt (Details: siehe Anlage)",
                "Auftrag abgelehnt (Details: siehe Anlage)",
            ),
            ("Averyveryveryverylongwordthatcouldbeatoken ok", "# ok"),
            ("Siehe www.beispiel.de und mail.example", "Siehe # und #"),
            (
                "BIC COBADEFF oder COBADEFFXXX genannt",
                "BIC # oder # genannt",
            ),
            ("Satz endet. Neuer Satz", "Satz endet. Neuer Satz"),
            ("HKTAN und SEPA bleiben", "HKTAN und SEPA bleiben"),
        ] {
            assert_eq!(redact_notice(text), expected, "{text}");
        }
        let long = "wort ".repeat(100);
        assert_eq!(redact_notice(&long).chars().count(), NOTICE_MAX_CHARS);
    }

    #[test]
    fn a_notice_line_has_no_character_that_needs_escaping() {
        let at: Timestamp = "2026-11-06T16:00:00Z".parse().unwrap();
        let text = notice_line(at, 9160, "Fehler \"x\" \\ DE89370400440532013000 \u{1F600}");
        assert_eq!(
            text,
            "{\"at\":\"2026-11-06T16:00:00.000Z\",\"step\":\"bank.bank-notice\",\"n\":9160,\"text\":\"Fehler x #\"}\n"
        );
        assert_eq!(text.matches('\n').count(), 1);
    }

    #[test]
    fn names_are_unique_and_shaped_like_codes() {
        let mut seen = std::collections::HashSet::new();
        for step in Step::ALL {
            assert!(is_error_code(step.name()), "{}", step.name());
            assert!(seen.insert(step.name()), "{}", step.name());
        }
    }
}
