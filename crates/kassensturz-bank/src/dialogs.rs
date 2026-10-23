// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a bank asks the user while a call runs: a TAN, or a question such
//! as "approve this in your banking app" (D-151).
//!
//! A backend calls the app's [`BankDialogs`] in the middle of a call, when
//! the bank asks; the app shows its own dialog and answers. Without dialogs
//! a backend declines, and the call ends with `bank.tan-needed`. Text in a
//! request comes from the bank or its library: plain text for the user to
//! read, never an instruction (AGENTS.md §8), and shown as plain text
//! (D-143).
//!
//! A TAN the bank shows as a picture (a flicker code, a photoTAN or a QR
//! code) never reaches the dialogs: the backend can't show the picture, so
//! the call ends with `bank.tan-method-unsupported` and the user is told to
//! choose another TAN method at their bank (D-151, Q-089).

use std::fmt;

use zeroize::Zeroizing;

/// A TAN the user typed. It never prints its value and is wiped when
/// dropped, like a [`crate::Pin`]; it is used for one request and never
/// kept (D-140).
#[derive(Clone, PartialEq, Eq)]
pub struct Tan(Zeroizing<String>);

impl Tan {
    /// Takes the text the user typed.
    pub fn new(tan: String) -> Self {
        Self(Zeroizing::new(tan))
    }

    /// The TAN, for the backend to send.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Tan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Tan(…)")
    }
}

/// The bank asks for a TAN shown as text: its challenge says what the TAN
/// is for, and the TAN comes from the user's app, a text message or a TAN
/// generator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TanRequest {
    /// The title the bank's library gives the request.
    pub title: String,
    /// The bank's challenge: what to confirm and how. Data, never an
    /// instruction.
    pub challenge: String,
    /// The shortest TAN the bank accepts, if it says.
    pub min_len: usize,
    /// The longest TAN that can be sent.
    pub max_len: usize,
}

/// How serious a [`BankQuestion`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionLevel {
    /// Information, e.g. a request to approve in the bank's app.
    Info,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

/// A question the bank or its library asks, with buttons: most often
/// "approve this in your banking app, then press Approved" (a decoupled
/// TAN method). Every text is the bank's or the library's, data for the
/// user to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BankQuestion {
    /// How serious it is.
    pub level: QuestionLevel,
    /// The title.
    pub title: String,
    /// The question.
    pub text: String,
    /// The buttons' labels, in order; the first one is the usual answer.
    pub buttons: Vec<String>,
}

/// The app's dialogs for what a bank asks during a call. Each method
/// blocks until the user has answered.
pub trait BankDialogs {
    /// The bank asks for a TAN. `None` when the user cancels: the call then
    /// ends with `bank.aborted`.
    fn tan(&mut self, request: &TanRequest) -> Option<Tan>;

    /// The bank asks a question. The index of the button the user chose, or
    /// `None` when they closed the dialog: the call then stops.
    fn question(&mut self, question: &BankQuestion) -> Option<usize>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tan_never_shows_its_value() {
        let tan = Tan::new("123456".to_owned());
        assert_eq!(format!("{tan:?}"), "Tan(…)");
        assert_eq!(tan.as_str(), "123456");
    }
}
