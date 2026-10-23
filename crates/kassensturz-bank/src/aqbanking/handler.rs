// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What answers AqBanking's callbacks for the backend (D-140).
//!
//! Before a call that may talk to the bank, the backend puts the PIN the
//! [`crate::PinSession`] allows for the login into the handler; the
//! handler hands it to AqBanking when asked and nothing else, and records
//! what happened: a request it had to decline, a PIN the bank called
//! wrong, and the messages the bank or the backend logged, so the backend
//! can turn them into error codes (D-034). After the call the PIN is
//! dropped and wiped.
//!
//! A TAN request is declined here: the dialogs for it come with M4.6, and
//! the backend tells the user so (`bank.tan-needed`).

use kassensturz_aqbanking_sys::session::{
    LogLevel, PasswordAnswer, PasswordKind, PasswordRequest, PasswordStatus, SessionHandler,
};
use zeroize::Zeroizing;

use crate::pin::Pin;

/// How many log lines are kept for the error text.
const KEPT_LINES: usize = 20;

#[derive(Default)]
pub(crate) struct Handler {
    /// The PIN the running call may send, if any.
    pin: Option<Pin>,
    /// The bank called the PIN wrong during the running call.
    pub rejected: bool,
    /// The last log lines of notice level and above: the bank's messages
    /// and the backend's warnings, for the error text.
    lines: Vec<String>,
}

impl Handler {
    /// Prepares the running call: what PIN it may send, and a clean
    /// record of what happens.
    pub fn begin(&mut self, pin: Option<Pin>) {
        self.pin = pin;
        self.rejected = false;
        self.lines.clear();
    }

    /// Ends the running call: the PIN is dropped and wiped.
    pub fn end(&mut self) {
        self.pin = None;
    }

    /// The last message of the bank or the backend, for `bank.refused`
    /// and `bank.setup-failed`.
    pub fn last_message(&self) -> Option<String> {
        self.lines.last().cloned()
    }
}

impl SessionHandler for Handler {
    fn password(&mut self, request: &PasswordRequest) -> PasswordAnswer {
        match request.kind {
            PasswordKind::Pin => match &self.pin {
                Some(pin) => PasswordAnswer::Text(Zeroizing::new(pin.as_str().to_owned())),
                None => PasswordAnswer::Decline,
            },
            PasswordKind::Tan | PasswordKind::Other => PasswordAnswer::Decline,
        }
    }

    fn password_status(&mut self, status: PasswordStatus) {
        if status == PasswordStatus::Bad {
            self.rejected = true;
        }
    }

    fn log(&mut self, level: LogLevel, message: &str) {
        if level < LogLevel::Notice || message.trim().is_empty() {
            return;
        }
        if self.lines.len() == KEPT_LINES {
            self.lines.remove(0);
        }
        self.lines.push(message.trim().to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: PasswordKind) -> PasswordRequest {
        PasswordRequest {
            kind,
            title: "Enter PIN".to_owned(),
            text: "Please enter the PIN".to_owned(),
            min_len: 4,
            max_len: 64,
        }
    }

    #[test]
    fn the_pin_is_sent_only_while_a_call_runs_and_only_for_a_pin() {
        let mut handler = Handler::default();
        assert!(matches!(
            handler.password(&request(PasswordKind::Pin)),
            PasswordAnswer::Decline
        ));
        handler.begin(Some(Pin::new("0000".to_owned())));
        match handler.password(&request(PasswordKind::Pin)) {
            PasswordAnswer::Text(text) => assert_eq!(text.as_str(), "0000"),
            PasswordAnswer::Decline => panic!("the PIN is known"),
        }
        assert!(matches!(
            handler.password(&request(PasswordKind::Tan)),
            PasswordAnswer::Decline
        ));
        assert!(matches!(
            handler.password(&request(PasswordKind::Other)),
            PasswordAnswer::Decline
        ));
        handler.end();
        assert!(matches!(
            handler.password(&request(PasswordKind::Pin)),
            PasswordAnswer::Decline
        ));
    }

    #[test]
    fn a_bad_pin_and_the_last_message_are_recorded() {
        let mut handler = Handler::default();
        handler.begin(None);
        handler.password_status(PasswordStatus::Ok);
        assert!(!handler.rejected);
        handler.password_status(PasswordStatus::Bad);
        assert!(handler.rejected);
        handler.log(LogLevel::Debug, "noise");
        handler.log(LogLevel::Notice, "  Bank says: no  ");
        handler.log(LogLevel::Warning, "");
        assert_eq!(handler.last_message().as_deref(), Some("Bank says: no"));
        for index in 0..KEPT_LINES + 5 {
            handler.log(LogLevel::Error, &format!("line {index}"));
        }
        assert_eq!(handler.lines.len(), KEPT_LINES);
        assert_eq!(
            handler.last_message().as_deref(),
            Some(format!("line {}", KEPT_LINES + 4).as_str())
        );
        handler.begin(None);
        assert_eq!(handler.last_message(), None);
        assert!(!handler.rejected);
    }
}
