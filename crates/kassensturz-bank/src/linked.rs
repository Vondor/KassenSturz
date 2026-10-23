// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fetching the statement of an account that is linked to a bank account
//! (M4.7, D-148, D-153): which of the logins' bank accounts the link names,
//! the fetch from the day of the last one, the PIN asked again when the
//! bank needs it, and the conversion into a statement. The front end only
//! brings the link and the answers of the user; what the fetch comes to is
//! decided here, once, for the screens and for the tests.

use kassensturz_core::{BankLink, Date};
use kassensturz_import::{Limits, Statement};

use crate::backend::{Bank, BankAccount, Login};
use crate::error::BankError;
use crate::pin::{Pin, PinMode, PinSession};
use crate::statement::statement;

/// How many times the bank may call a PIN wrong in one fetch before the fetch
/// stops asking. Banks block a login after a few wrong PINs (threat T-05), so
/// the user is asked once more after the first, and never a third time.
const MAX_REJECTED_PINS: usize = 2;

/// How the front end asks the user for the PIN of a login: a dialog, or in a
/// test an answer made up beforehand.
pub trait PinAsker {
    /// The PIN the user typed for `login` and how long to keep it, or `None`
    /// when the user cancelled. `rejected` says the bank has just called the
    /// last PIN wrong, so the dialog can say so.
    fn pin(&mut self, login: &Login, rejected: bool) -> Option<(Pin, PinMode)>;
}

/// The text without spaces and in capitals, the way IBANs and account
/// numbers are compared, whichever way the user, the book or the bank wrote
/// them.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

/// Whether `account` of `login` is the bank account `link` names: the same
/// IBAN when both have one, otherwise the same bank code and number (D-148).
/// Two different IBANs never match through their numbers.
pub fn names_account(link: &BankLink, login: &Login, account: &BankAccount) -> bool {
    let wanted = link
        .iban
        .as_deref()
        .map(plain)
        .filter(|iban| !iban.is_empty());
    let listed = account
        .iban
        .as_deref()
        .map(plain)
        .filter(|iban| !iban.is_empty());
    if let (Some(wanted), Some(listed)) = (&wanted, &listed) {
        return wanted == listed;
    }
    let (Some(code), Some(number)) = (link.bank_code.as_deref(), link.number.as_deref()) else {
        return false;
    };
    let number = plain(number);
    !number.is_empty()
        && plain(code) == plain(login.bank.code())
        && account.number.as_deref().map(plain).as_deref() == Some(number.as_str())
}

/// The bank account `link` names, among the accounts the logins list, and
/// the login it is reached through. The first match wins: two logins that
/// list the same account reach the same account at the bank. Fails with
/// `bank.no-bank-account` when none does.
pub fn find_bank_account(
    bank: &dyn Bank,
    link: &BankLink,
) -> Result<(Login, BankAccount), BankError> {
    for login in bank.logins()? {
        let accounts = bank.accounts(&login.id)?;
        if let Some(account) = accounts
            .into_iter()
            .find(|account| names_account(link, &login, account))
        {
            return Ok((login, account));
        }
    }
    Err(BankError::NoBankAccount)
}

/// How many days before the last fetched day a fetch starts (Q-090, D-153):
/// a bank may book a transaction a day or two late, with the booking date of
/// the day before, so a fetch that began on the last fetched day itself
/// would miss it. What the book has already is skipped by the import
/// pipeline's duplicate check (D-030), so the overlap adds nothing twice.
pub const FETCH_OVERLAP_DAYS: u8 = 5;

/// The day a fetch asks the bank from, given the day of the last fetch:
/// [`FETCH_OVERLAP_DAYS`] days earlier, or `None` (as far back as the bank
/// goes) for an account never fetched and, since a day cannot be before the
/// first one the calendar has, when the subtraction has no result.
pub fn fetch_start(last_fetched_on: Option<Date>) -> Option<Date> {
    let mut day = last_fetched_on?;
    for _ in 0..FETCH_OVERLAP_DAYS {
        day = day.yesterday().ok()?;
    }
    Some(day)
}

/// Fetches what the bank has for the account `link` names, from
/// [`FETCH_OVERLAP_DAYS`] days before the day of the last fetch on (see
/// [`fetch_start`]), or as far back as the bank goes at the first fetch, and
/// turns it into a statement within `limits`. A PIN the bank needs is asked
/// through `asker`; nothing else is asked here (a TAN comes through the
/// backend's dialogs, D-151).
///
/// This changes nothing in the book: the statement goes through the same
/// import pipeline as a file, and the day is noted there, in the import's
/// own action (`Book::note_bank_fetch`).
pub fn fetch_linked(
    bank: &mut dyn Bank,
    pins: &mut PinSession,
    asker: &mut dyn PinAsker,
    link: &BankLink,
    limits: &Limits,
) -> Result<Statement, BankError> {
    let (login, account) = find_bank_account(bank, link)?;
    let mut rejected = 0;
    loop {
        match bank.fetch(&account.id, fetch_start(link.last_fetched_on), pins) {
            Ok(fetched) => return statement(&fetched, limits),
            Err(BankError::PinNeeded) => {}
            Err(BankError::PinRejected) if rejected + 1 < MAX_REJECTED_PINS => rejected += 1,
            Err(error) => return Err(error),
        }
        let (pin, mode) = asker.pin(&login, rejected > 0).ok_or(BankError::Aborted)?;
        pins.provide(&login.id, pin, mode)?;
    }
}

#[cfg(test)]
mod tests {
    use kassensturz_core::{AccountId, Date};

    use super::*;
    use crate::backend::LoginId;
    use crate::fake::{CURRENT_ACCOUNT_IBAN, FakeBank};
    use crate::pin::MemoryPinStore;

    /// A fake bank with one login, whose PIN the session has forgotten again,
    /// so the next fetch has to ask.
    fn bank() -> (FakeBank, PinSession, Login) {
        let mut bank = FakeBank::new();
        let mut pins = PinSession::new(Box::new(MemoryPinStore::new()));
        pins.provide(
            &LoginId::pending(),
            Pin::new(FakeBank::PIN.to_owned()),
            PinMode::RememberUntilClose,
        )
        .expect("provided");
        let id = bank
            .add_login(&FakeBank::setup(), &mut pins)
            .expect("login added");
        pins.forget(&id).expect("forgotten");
        let login = bank.logins().expect("logins").remove(0);
        (bank, pins, login)
    }

    fn link(iban: Option<&str>, bank_code: Option<&str>, number: Option<&str>) -> BankLink {
        BankLink {
            account: AccountId::generate(),
            iban: iban.map(str::to_owned),
            bank_code: bank_code.map(str::to_owned),
            number: number.map(str::to_owned),
            last_fetched_on: None,
        }
    }

    /// Answers with the PINs it is given in turn, and remembers whether it
    /// was told the last one was wrong.
    struct Answers {
        pins: Vec<Option<&'static str>>,
        mode: PinMode,
        asked: Vec<bool>,
    }

    impl Answers {
        fn new(pins: &[Option<&'static str>]) -> Self {
            Self {
                pins: pins.iter().rev().copied().collect(),
                mode: PinMode::AskEachTime,
                asked: Vec::new(),
            }
        }
    }

    impl PinAsker for Answers {
        fn pin(&mut self, _login: &Login, rejected: bool) -> Option<(Pin, PinMode)> {
            self.asked.push(rejected);
            let pin = self.pins.pop().expect("asked more often than answered")?;
            Some((Pin::new(pin.to_owned()), self.mode))
        }
    }

    #[test]
    fn a_link_finds_its_bank_account_by_iban_however_it_is_written() {
        let (bank, _, login) = bank();
        for iban in [
            CURRENT_ACCOUNT_IBAN,
            "DE89 3704 0044 0532 0130 00",
            "de89370400440532013000",
        ] {
            let (found_login, account) =
                find_bank_account(&bank, &link(Some(iban), None, None)).expect("found");
            assert_eq!(found_login, login, "{iban}");
            assert_eq!(account.iban.as_deref(), Some(CURRENT_ACCOUNT_IBAN));
        }
    }

    #[test]
    fn a_link_without_an_iban_finds_its_bank_account_by_bank_code_and_number() {
        let (bank, _, login) = bank();
        let code = login.bank.code().to_owned();
        let (_, card) =
            find_bank_account(&bank, &link(None, Some(&code), Some("4000 0000 0000 0001")))
                .expect("found");
        assert_eq!(card.name.as_deref(), Some("Credit card"));
        // The IBAN of a link that has one wins; the number is not asked.
        let (_, current) = find_bank_account(
            &bank,
            &link(
                Some(CURRENT_ACCOUNT_IBAN),
                Some(&code),
                Some("4000000000000001"),
            ),
        )
        .expect("found");
        assert_eq!(current.iban.as_deref(), Some(CURRENT_ACCOUNT_IBAN));
    }

    #[test]
    fn a_link_that_names_no_listed_account_finds_none() {
        let (bank, _, login) = bank();
        let code = login.bank.code().to_owned();
        for wrong in [
            // Another IBAN, even with the number of a listed account.
            link(Some("BE68539007547034"), Some(&code), Some("0532013000")),
            // The right number at another bank.
            link(None, Some("10000000"), Some("4000 0000 0000 0001")),
            // A number without a bank code, or a code without a number.
            link(None, None, Some("4000 0000 0000 0001")),
            link(None, Some(&code), None),
            link(None, Some(&code), Some("  ")),
            link(None, None, None),
        ] {
            assert_eq!(
                find_bank_account(&bank, &wrong).map(|(_, account)| account.id),
                Err(BankError::NoBankAccount),
                "{wrong:?}"
            );
        }
    }

    #[test]
    fn without_a_login_no_bank_account_is_found() {
        let (mut bank, mut pins, login) = bank();
        bank.remove_login(&login.id).expect("removed");
        pins.forget(&login.id).expect("forgotten");
        assert_eq!(
            find_bank_account(&bank, &link(Some(CURRENT_ACCOUNT_IBAN), None, None))
                .map(|(_, account)| account.id),
            Err(BankError::NoBankAccount)
        );
    }

    #[test]
    fn the_fetch_starts_on_the_day_of_the_last_one() {
        let (mut bank, mut pins, _) = bank();
        let mut asker = Answers::new(&[Some(FakeBank::PIN)]);
        let mut everything = link(Some(CURRENT_ACCOUNT_IBAN), None, None);
        let all = fetch_linked(
            &mut bank,
            &mut pins,
            &mut asker,
            &everything,
            &Limits::default(),
        )
        .expect("a statement");
        assert_eq!(all.entries.len(), 4);
        // The PIN was asked once, and not remembered (ask each time).
        assert_eq!(asker.asked, vec![false]);
        assert_eq!(
            pins.answer(&LoginId::new("fake-login-1")),
            Ok(crate::pin::PinAnswer::Ask)
        );

        // The last fetch was on the 10th, so this one starts five days
        // earlier, on the 5th (Q-090).
        everything.last_fetched_on = Some(Date::new(2026, 10, 10).expect("a date"));
        let mut asker = Answers::new(&[Some(FakeBank::PIN)]);
        let later = fetch_linked(
            &mut bank,
            &mut pins,
            &mut asker,
            &everything,
            &Limits::default(),
        )
        .expect("a statement");
        // The fake bank has the 5th and the (unbooked) 9th on or after it;
        // the start day itself is included, so nothing is missed.
        assert_eq!(later.entries.len(), 2);
        assert_eq!(later.entries[0].booking_date.to_string(), "2026-10-05");
    }

    #[test]
    fn a_fetch_starts_five_days_before_the_day_of_the_last_one() {
        let day = |year, month, d| Some(Date::new(year, month, d).expect("a date"));
        assert_eq!(FETCH_OVERLAP_DAYS, 5);
        assert_eq!(fetch_start(day(2026, 10, 23)), day(2026, 10, 18));
        // Across a month, a year and a leap day.
        assert_eq!(fetch_start(day(2026, 10, 3)), day(2026, 9, 28));
        assert_eq!(fetch_start(day(2027, 1, 2)), day(2026, 12, 28));
        assert_eq!(fetch_start(day(2028, 3, 2)), day(2028, 2, 26));
        // Never fetched: as far back as the bank goes.
        assert_eq!(fetch_start(None), None);
        // No day before the calendar's first one.
        assert_eq!(fetch_start(Some(Date::MIN)), None);
    }

    #[test]
    fn a_remembered_pin_is_not_asked_again() {
        let (mut bank, mut pins, _) = bank();
        let mut asker = Answers::new(&[Some(FakeBank::PIN)]);
        asker.mode = PinMode::RememberUntilClose;
        let link = link(Some(CURRENT_ACCOUNT_IBAN), None, None);
        for _ in 0..2 {
            fetch_linked(&mut bank, &mut pins, &mut asker, &link, &Limits::default())
                .expect("a statement");
            pins.end_session();
        }
        assert_eq!(
            asker.asked,
            vec![false],
            "the second fetch used the PIN kept"
        );
    }

    #[test]
    fn a_wrong_pin_is_asked_for_once_more_and_then_the_fetch_stops() {
        let (mut bank, mut pins, _) = bank();
        let link = link(Some(CURRENT_ACCOUNT_IBAN), None, None);

        // Wrong, then right: the dialog is told the first was wrong.
        let mut asker = Answers::new(&[Some("00000"), Some(FakeBank::PIN)]);
        fetch_linked(&mut bank, &mut pins, &mut asker, &link, &Limits::default())
            .expect("a statement");
        assert_eq!(asker.asked, vec![false, true]);

        // Wrong twice: the bank is not asked a third time, so the login is
        // not blocked (threat T-05).
        pins.forget(&LoginId::new("fake-login-1"))
            .expect("forgotten");
        let mut asker = Answers::new(&[Some("00000"), Some("11111"), Some(FakeBank::PIN)]);
        assert_eq!(
            fetch_linked(&mut bank, &mut pins, &mut asker, &link, &Limits::default()),
            Err(BankError::PinRejected)
        );
        assert_eq!(asker.asked, vec![false, true]);
    }

    #[test]
    fn cancelling_the_pin_dialog_ends_the_fetch() {
        let (mut bank, mut pins, _) = bank();
        let mut asker = Answers::new(&[None]);
        assert_eq!(
            fetch_linked(
                &mut bank,
                &mut pins,
                &mut asker,
                &link(Some(CURRENT_ACCOUNT_IBAN), None, None),
                &Limits::default()
            ),
            Err(BankError::Aborted)
        );
    }

    #[test]
    fn the_banks_errors_and_unreadable_amounts_end_the_fetch_unchanged() {
        let (mut bank, mut pins, _) = bank();
        let link = link(Some(CURRENT_ACCOUNT_IBAN), None, None);
        bank.fail_next(crate::fake::Failure::Unreachable);
        let mut asker = Answers::new(&[Some(FakeBank::PIN)]);
        assert_eq!(
            fetch_linked(&mut bank, &mut pins, &mut asker, &link, &Limits::default()),
            Err(BankError::Unreachable)
        );
        bank.fail_next(crate::fake::Failure::InexactAmount);
        let mut asker = Answers::new(&[Some(FakeBank::PIN)]);
        assert_eq!(
            fetch_linked(&mut bank, &mut pins, &mut asker, &link, &Limits::default()),
            Err(BankError::InexactAmount {
                amount: "1/3".to_owned()
            })
        );
    }
}
