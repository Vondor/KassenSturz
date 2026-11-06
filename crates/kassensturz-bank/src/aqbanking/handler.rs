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
//! A TAN request and a question, such as an approval in the bank's app,
//! go to the app's [`BankDialogs`] when it gave the backend some (D-151);
//! the handler records why it declined one, so the backend can say so: no
//! dialogs (`bank.tan-needed`), a TAN shown as a picture it can't show
//! (`bank.tan-method-unsupported`), or the user cancelled (`bank.aborted`).

use kassensturz_aqbanking_sys::session::{
    LogLevel, PasswordAnswer, PasswordKind, PasswordMethod, PasswordRequest, PasswordStatus,
    QuestionLevel as SessionLevel, QuestionRequest, SessionHandler,
};
use zeroize::Zeroizing;

use crate::dialogs::{BankDialogs, BankQuestion, QuestionLevel, TanRequest};
use crate::pin::Pin;

/// Why the handler declined what the bank asked during a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The bank showed a TAN as a picture.
    PictureTan,
    /// The user cancelled a TAN or closed a question.
    Cancelled,
}

/// How many log lines are kept for the error text.
const KEPT_LINES: usize = 20;

/// The most return codes kept for one call.
const KEPT_CODES: usize = 8;

/// The warning and error codes of the FinTS specification that the bank
/// log may name (D-607). A fixed list: a number in a bank's text that is not
/// on it is never taken. The success codes (below 1000) are left out, since
/// short numbers are too likely to be an amount.
pub(crate) const REPLY_CODES: &[u16] = &[
    3040, 3050, 3060, 3076, 3920, 3955, 3956, 9000, 9010, 9050, 9075, 9110, 9120, 9130, 9140, 9160,
    9210, 9220, 9230, 9250, 9340, 9800, 9801, 9810, 9930, 9931, 9932, 9942, 9955, 9956,
];

/// The return code of a result line of AqBanking, which has the form
/// `NNNN - text`: the line's **first** number must be four digits that
/// stand alone and are followed by ` - `, and be on the list. Any other
/// number in the line is never looked at, so a limit (`9000 EUR`), a date,
/// an IBAN group or an amount can't be taken for a code (D-607).
pub(crate) fn reply_code(line: &str) -> Option<u16> {
    let digits = line.find(|character: char| character.is_ascii_digit())?;
    let rest = &line[digits..];
    let length = rest
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(rest.len());
    let before_is_plain = line[..digits]
        .chars()
        .next_back()
        .is_none_or(|character| character.is_whitespace() || "([:;".contains(character));
    if length != 4 || !before_is_plain || !rest[length..].starts_with(" - ") {
        return None;
    }
    rest[..length]
        .parse::<u16>()
        .ok()
        .filter(|code| REPLY_CODES.contains(code))
}

#[derive(Default)]
pub(crate) struct Handler {
    /// The PIN the running call may send, if any.
    pin: Option<Pin>,
    /// The app's dialogs for TANs and questions, if it gave some.
    pub dialogs: Option<Box<dyn BankDialogs>>,
    /// The bank called the PIN wrong during the running call.
    pub rejected: bool,
    /// Why the handler declined a request of the running call, if it did.
    pub refusal: Option<Refusal>,
    /// The last log lines of notice level and above: the bank's messages
    /// and the backend's warnings, for the error text.
    lines: Vec<String>,
    /// The bank's return codes of the running call (D-607), each once.
    codes: Vec<u16>,
}

impl Handler {
    /// The bank's return codes seen during the last call, each once, in
    /// the order they came (D-607).
    pub fn reply_codes(&self) -> &[u16] {
        &self.codes
    }

    /// Prepares the running call: what PIN it may send, and a clean
    /// record of what happens.
    pub fn begin(&mut self, pin: Option<Pin>) {
        self.pin = pin;
        self.rejected = false;
        self.refusal = None;
        self.lines.clear();
        self.codes.clear();
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

impl Handler {
    /// Asks the user for a TAN through the dialogs, if the bank shows it as
    /// text and there are dialogs.
    fn tan(&mut self, request: &PasswordRequest) -> PasswordAnswer {
        if request.method == PasswordMethod::Optical {
            self.refusal = Some(Refusal::PictureTan);
            return PasswordAnswer::Decline;
        }
        let Some(dialogs) = self.dialogs.as_mut() else {
            return PasswordAnswer::Decline;
        };
        match dialogs.tan(&TanRequest {
            title: request.title.clone(),
            challenge: request.text.clone(),
            min_len: request.min_len,
            max_len: request.max_len,
        }) {
            Some(tan) => PasswordAnswer::Text(Zeroizing::new(tan.as_str().to_owned())),
            None => {
                self.refusal = Some(Refusal::Cancelled);
                PasswordAnswer::Decline
            }
        }
    }
}

impl SessionHandler for Handler {
    fn password(&mut self, request: &PasswordRequest) -> PasswordAnswer {
        match request.kind {
            PasswordKind::Pin => match &self.pin {
                Some(pin) => PasswordAnswer::Text(Zeroizing::new(pin.as_str().to_owned())),
                None => PasswordAnswer::Decline,
            },
            PasswordKind::Tan => self.tan(request),
            PasswordKind::Other => PasswordAnswer::Decline,
        }
    }

    fn question(&mut self, question: &QuestionRequest) -> Option<usize> {
        let dialogs = self.dialogs.as_mut()?;
        let answer = dialogs.question(&BankQuestion {
            level: match question.level {
                SessionLevel::Info => QuestionLevel::Info,
                SessionLevel::Warning => QuestionLevel::Warning,
                SessionLevel::Error => QuestionLevel::Error,
            },
            title: question.title.clone(),
            text: question.text.clone(),
            buttons: question.buttons.clone(),
        });
        if answer.is_none() {
            self.refusal = Some(Refusal::Cancelled);
        }
        answer
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
        // Only the result lines of notice level and above (D-607).
        if let Some(code) = reply_code(message.trim())
            && self.codes.len() < KEPT_CODES
            && !self.codes.contains(&code)
        {
            self.codes.push(code);
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
            method: PasswordMethod::Text,
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
    fn a_return_code_is_taken_only_from_a_result_line() {
        // D-607: the first number of the line, four digits, then " - ".
        assert_eq!(reply_code("9931 - Login or PIN wrong"), Some(9931));
        assert_eq!(reply_code("HBCI: 9942 - PIN locked"), Some(9942));
        assert_eq!(reply_code("(9075 - strong authentication)"), Some(9075));
        // A limit, an amount, a date, an IBAN group, a longer number.
        assert_eq!(reply_code("limit 9000 EUR"), None);
        assert_eq!(reply_code("balance 9000,00 - EUR"), None);
        assert_eq!(
            reply_code("on 2026-11-06 9931 - x"),
            None,
            "first number is the year"
        );
        assert_eq!(reply_code("DE89 3704 0044 - x"), None);
        assert_eq!(reply_code("99319931 - x"), None);
        // Not on the list, or no number at all.
        assert_eq!(reply_code("0020 - fine"), None);
        assert_eq!(reply_code("no digits here"), None);
    }

    #[test]
    fn a_calls_return_codes_are_kept_once_and_cleared_by_the_next_call() {
        let mut handler = Handler::default();
        handler.begin(None);
        // Below notice level, nothing is taken.
        handler.log(LogLevel::Info, "9800 - not kept");
        handler.log(LogLevel::Notice, "9931 - Login or PIN wrong");
        handler.log(LogLevel::Notice, "9931 - again");
        handler.log(LogLevel::Notice, "9942 - locked");
        handler.log(LogLevel::Notice, "limit 9000 EUR");
        assert_eq!(handler.reply_codes(), &[9931, 9942]);
        handler.begin(None);
        assert!(handler.reply_codes().is_empty());
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

    /// Dialogs that answer with what they were given and record the
    /// requests.
    struct Answers {
        tan: Option<&'static str>,
        button: Option<usize>,
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl BankDialogs for Answers {
        fn tan(&mut self, request: &TanRequest) -> Option<crate::Tan> {
            self.seen
                .borrow_mut()
                .push(format!("tan: {}", request.challenge));
            self.tan.map(|tan| crate::Tan::new(tan.to_owned()))
        }

        fn question(&mut self, question: &BankQuestion) -> Option<usize> {
            self.seen
                .borrow_mut()
                .push(format!("question: {} {:?}", question.text, question.level));
            self.button
        }
    }

    fn with_dialogs(
        tan: Option<&'static str>,
        button: Option<usize>,
    ) -> (Handler, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
        let seen = std::rc::Rc::default();
        let handler = Handler {
            dialogs: Some(Box::new(Answers {
                tan,
                button,
                seen: std::rc::Rc::clone(&seen),
            })),
            ..Handler::default()
        };
        (handler, seen)
    }

    fn question() -> QuestionRequest {
        QuestionRequest {
            level: SessionLevel::Warning,
            title: "Approval".to_owned(),
            text: "Approve in your app".to_owned(),
            buttons: vec!["Approved".to_owned(), "Abort".to_owned()],
        }
    }

    #[test]
    fn a_tan_and_a_question_go_to_the_dialogs() {
        let (mut handler, seen) = with_dialogs(Some("123456"), Some(0));
        handler.begin(None);
        match handler.password(&PasswordRequest {
            text: "Challenge 4711".to_owned(),
            ..request(PasswordKind::Tan)
        }) {
            PasswordAnswer::Text(text) => assert_eq!(text.as_str(), "123456"),
            PasswordAnswer::Decline => panic!("the dialogs gave a TAN"),
        }
        assert_eq!(handler.question(&question()), Some(0));
        assert_eq!(handler.refusal, None);
        assert_eq!(
            *seen.borrow(),
            vec![
                "tan: Challenge 4711".to_owned(),
                "question: Approve in your app Warning".to_owned()
            ]
        );
    }

    #[test]
    fn why_a_request_was_declined_is_recorded() {
        // Without dialogs: declined, and nothing recorded, so the session's
        // own error says "TAN needed".
        let mut handler = Handler::default();
        handler.begin(None);
        assert!(matches!(
            handler.password(&request(PasswordKind::Tan)),
            PasswordAnswer::Decline
        ));
        assert_eq!(handler.question(&question()), None);
        assert_eq!(handler.refusal, None);
        // A TAN as a picture never reaches the dialogs.
        let (mut handler, seen) = with_dialogs(Some("123456"), None);
        handler.begin(None);
        assert!(matches!(
            handler.password(&PasswordRequest {
                method: PasswordMethod::Optical,
                ..request(PasswordKind::Tan)
            }),
            PasswordAnswer::Decline
        ));
        assert_eq!(handler.refusal, Some(Refusal::PictureTan));
        assert!(seen.borrow().is_empty());
        // The user cancels.
        handler.begin(None);
        assert_eq!(handler.refusal, None);
        assert_eq!(handler.question(&question()), None);
        assert_eq!(handler.refusal, Some(Refusal::Cancelled));
        let (mut handler, _) = with_dialogs(None, None);
        handler.begin(None);
        assert!(matches!(
            handler.password(&request(PasswordKind::Tan)),
            PasswordAnswer::Decline
        ));
        assert_eq!(handler.refusal, Some(Refusal::Cancelled));
    }
}
