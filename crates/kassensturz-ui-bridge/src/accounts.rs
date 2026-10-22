// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The accounts screen (M3.3): the accounts side by side with their balances
//! per currency (D-021, D-067), adding, renaming and closing them (D-081),
//! and "Check against my bank" (D-029, D-111).
//!
//! The screen reads the open book directly, through the public functions of
//! core and store, so every balance is the real one. Everything it shows is
//! text formatted here (D-065); what the user types comes back as text and is
//! read here too. Accounts are named to QML by their ID as text, since their
//! place in a list changes when one is added or closed.

use std::collections::HashSet;

use kassensturz_core::{
    Account, AccountId, AccountKind, AccountType, CurrencyCode, Date, Money, Report, Split,
    Transaction, TransactionError, TransactionId,
};
use kassensturz_store::{AccountPart, Action, Book, StatedBalance, StoreError};

use crate::errors::{ErrorText, report_text};
use crate::first_run::type_key;
use crate::format::Locale;
use crate::parse::{AmountInputError, AmountReader};
use crate::register::{AmountWriter, Shown};

/// Why something the user entered wasn't accepted. The UI shows a message for
/// each code (D-034).
#[derive(Debug)]
pub enum AccountsError {
    /// What the user typed as an amount isn't one, in this currency.
    Amount(AmountInputError, CurrencyCode),
    /// What a credit card owes can't be typed below zero.
    NegativeDebt,
    /// What the user typed as a day isn't one.
    Date,
    /// The day hasn't come yet.
    DateInFuture,
    /// The currency isn't one KassenSturz lists for new accounts.
    UnknownCurrency,
    /// There is no such account, e.g. it was just changed elsewhere.
    NoSuchEntry,
    /// The starting balance can't be corrected from this day, since one is
    /// dated after it and would then count twice.
    StartingBalanceLater,
    /// The book refused it, e.g. a name that is taken.
    Store(StoreError),
    /// The starting balance doesn't add up; only an amount too large can get
    /// here.
    Transaction(TransactionError),
}

impl From<StoreError> for AccountsError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// An account as the accounts screen lists it.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountCard {
    /// The account's ID as text, to name it to QML.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its type, as a key the UI translates: `checking`, `savings`, `cash`
    /// or `credit-card`; empty for an account made before types existed.
    pub account_type: &'static str,
    /// Its balance; for a debt, what the user owes.
    pub balance: Shown,
    /// Whether the balance is shown as "You owe" (D-067).
    pub owes: bool,
}

/// The open accounts in one currency, with what they come to together.
#[derive(Debug, PartialEq, Eq)]
pub struct CurrencySection {
    /// The ISO 4217 code.
    pub currency: String,
    /// The accounts, in the order they were added.
    pub accounts: Vec<AccountCard>,
    /// What the user has minus what they owe, added up from the exact
    /// balances (D-002).
    pub together: Shown,
    /// Whether, all together, the user owes money.
    pub together_owes: bool,
}

/// What the accounts screen shows.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountsView {
    /// The open accounts per currency, the currency used first first.
    /// Amounts of different currencies are never added up.
    pub sections: Vec<CurrencySection>,
    /// The closed accounts, which the screen lists on request.
    pub closed: Vec<AccountCard>,
}

/// One account in detail.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountDetails {
    /// The account as it is listed.
    pub card: AccountCard,
    /// Its currency.
    pub currency: String,
    /// Whether it is closed.
    pub closed: bool,
    /// Whether it is a debt, i.e. a credit card.
    pub debt: bool,
    /// Whether it can be closed now: nothing is left in it (D-081).
    pub can_close: bool,
    /// What the user said the bank showed, if they said (D-117).
    pub stated: Option<Statement>,
}

/// The balance the user stated for an account, taken from the bank, against
/// what KassenSturz has for that day (D-111, D-117).
#[derive(Debug, PartialEq, Eq)]
pub struct Statement {
    /// What the bank showed; for a debt, what was owed. Its exact text is
    /// what the user could type into "Check against my bank".
    pub balance: Shown,
    /// Whether that is shown as "owed".
    pub owes: bool,
    /// The day it was for, in the user's short format, which "Check against
    /// my bank" reads back.
    pub date: String,
    /// What KassenSturz has at the end of that day; for a debt, what is owed.
    pub held: Shown,
    /// Whether that is shown as "owed".
    pub held_owes: bool,
    /// Whether the two agree.
    pub matches: bool,
}

/// An account as the user entered it.
pub struct NewAccount<'a> {
    /// The name, as typed.
    pub name: &'a str,
    /// What kind of account it is.
    pub account_type: AccountType,
    /// The ISO 4217 code of its currency.
    pub currency: &'a str,
    /// How much is in it on `date` (for a credit card, how much is owed), as
    /// typed. Nothing typed means zero.
    pub balance: &'a str,
    /// The day the balance is for, as typed. Nothing typed means today.
    pub date: &'a str,
}

/// The result of "Check against my bank".
#[derive(Debug, PartialEq, Eq)]
pub struct BankCheck {
    /// The day checked.
    pub date: String,
    /// What the bank says; for a debt, what is owed.
    pub bank: Shown,
    /// Whether the bank's balance is shown as "You owe".
    pub bank_owes: bool,
    /// What KassenSturz has at the end of that day.
    pub book: Shown,
    /// Whether KassenSturz's balance is shown as "You owe".
    pub book_owes: bool,
    /// Whether the account is a debt, so the texts speak of what is owed.
    pub debt: bool,
    /// Whether both say the same.
    pub matches: bool,
    /// The difference, without a sign.
    pub difference: Shown,
    /// Whether KassenSturz's balance is the higher one in money terms: more
    /// in the account, or for a debt, less owed.
    pub book_higher: bool,
    /// Where the difference can come from, most specific first. Each is a
    /// code with parameters, for the UI to write (D-034).
    pub explanations: Vec<ErrorText>,
    /// The correction of the starting balance KassenSturz offers, if the two
    /// differ and the account's starting balance isn't dated after the day
    /// (D-111, D-116). The user confirms it; nothing is changed here.
    pub correction: Option<Correction>,
}

/// What correcting the starting balance would do, so the user can decide.
#[derive(Debug, PartialEq, Eq)]
pub struct Correction {
    /// What the starting balance becomes; for a debt, what is owed.
    pub would_be: Shown,
    /// Whether that is shown as "owed".
    pub owes: bool,
    /// Whether a starting balance is added, since the account has none dated
    /// on or before the day; otherwise the latest one before the day changes.
    pub adds: bool,
    /// The day the starting balance is dated.
    pub date: String,
}

/// The starting balances of an account (the parts of transactions against
/// the internal account) and its other entries, each with its transaction's
/// payee.
type PartsByOrigin = (Vec<AccountPart>, Vec<(AccountPart, String)>);

/// What `account` holds, split into its starting balances and the rest.
fn parts_by_origin(book: &Book, account: &Account) -> Result<PartsByOrigin, StoreError> {
    let equity: HashSet<AccountId> = book
        .all_accounts()?
        .into_iter()
        .filter(|other| other.kind == AccountKind::Equity)
        .map(|other| other.id)
        .collect();
    let (mut starting, mut entries) = (Vec::new(), Vec::new());
    for part in book.account_parts(account.id)? {
        let transaction = book.transaction(part.transaction)?;
        let is_starting = transaction
            .splits()
            .iter()
            .any(|split| split.account.is_some_and(|other| equity.contains(&other)));
        let payee = transaction.payee().unwrap_or_default().to_owned();
        if is_starting {
            starting.push(part);
        } else {
            entries.push((part, payee));
        }
    }
    Ok((starting, entries))
}

/// The starting balance a correction on `day` changes: the latest one dated
/// on or before it. `Err(())` if one is dated after the day, since adding or
/// changing an earlier one would then count twice.
fn correctable(starting: &[AccountPart], day: Date) -> Result<Option<&AccountPart>, ()> {
    if starting.iter().any(|part| part.bank_date() > day) {
        return Err(());
    }
    Ok(starting
        .iter()
        .filter(|part| part.bank_date() <= day)
        .max_by_key(|part| (part.bank_date(), part.transaction)))
}

/// What the book holds about the accounts, for the accounts screen.
pub fn view(book: &Book, locale: &impl Locale) -> Result<AccountsView, StoreError> {
    let mut sections: Vec<(CurrencyCode, Vec<AccountCard>, Vec<Money>)> = Vec::new();
    let mut closed = Vec::new();
    // Sorted by ID, so in the order they were added (D-067).
    for account in book.all_accounts()? {
        if !account.kind.is_account() {
            continue;
        }
        let balance = book.balance(account.id)?;
        let writer = AmountWriter::new(book, account.currency, locale)?;
        let card = card(&account, balance, &writer);
        if account.closed {
            closed.push(card);
            continue;
        }
        let index = match sections
            .iter()
            .position(|(currency, ..)| *currency == account.currency)
        {
            Some(index) => index,
            None => {
                sections.push((account.currency, Vec::new(), Vec::new()));
                sections.len() - 1
            }
        };
        sections[index].1.push(card);
        sections[index].2.push(balance);
    }
    let sections = sections
        .into_iter()
        .map(|(currency, accounts, balances)| {
            let together =
                Money::checked_sum(currency, balances).map_err(|_| StoreError::AmountOverflow)?;
            let writer = AmountWriter::new(book, currency, locale)?;
            let (together, together_owes) = writer.balance(together, AccountKind::Liability);
            Ok(CurrencySection {
                currency: currency.to_string(),
                accounts,
                together,
                together_owes,
            })
        })
        .collect::<Result<_, StoreError>>()?;
    Ok(AccountsView { sections, closed })
}

/// The currency the household mostly uses: that of the first open account
/// (the one added first); in a book without accounts, the currency the first
/// start chose, which its categories keep (D-116); otherwise `fallback`.
pub fn usual_currency(book: &Book, fallback: CurrencyCode) -> Result<CurrencyCode, StoreError> {
    Ok(currencies_in_use(book)?
        .first()
        .copied()
        .unwrap_or(fallback))
}

/// The currencies to offer for a new account: the ones in use first
/// ([`usual_currency`]'s rule, in the order they were added), or `fallback`
/// if there is none, then every other current one (ISO 4217), each once. The
/// first is the one to preselect.
pub fn currency_choices(
    book: &Book,
    fallback: CurrencyCode,
) -> Result<Vec<CurrencyCode>, StoreError> {
    let mut choices: Vec<CurrencyCode> = currencies_in_use(book)?
        .into_iter()
        .filter(|currency| currency.iso_minor_digits().is_some())
        .collect();
    if choices.is_empty() && fallback.iso_minor_digits().is_some() {
        choices.push(fallback);
    }
    let others: Vec<CurrencyCode> = CurrencyCode::iso_codes()
        .filter(|code| !choices.contains(code))
        .collect();
    choices.extend(others);
    Ok(choices)
}

/// The currencies the book uses, in the order they came into use: those of
/// the open accounts, or, in a book without accounts, those of the
/// categories, which keep the currency the first start chose (D-116).
fn currencies_in_use(book: &Book) -> Result<Vec<CurrencyCode>, StoreError> {
    // Sorted by ID, so in the order they were added (D-067).
    let all = book.all_accounts()?;
    let currencies_of = |pick: fn(&Account) -> bool| {
        let mut used: Vec<CurrencyCode> = Vec::new();
        for account in all.iter().filter(|account| pick(account)) {
            if !used.contains(&account.currency) {
                used.push(account.currency);
            }
        }
        used
    };
    let mut used = currencies_of(|account| account.kind.is_account() && !account.closed);
    if used.is_empty() {
        used = currencies_of(|account| account.kind.is_category());
    }
    Ok(used)
}

/// One account in detail.
pub fn details(
    book: &Book,
    id: &str,
    locale: &impl Locale,
) -> Result<AccountDetails, AccountsError> {
    let account = find(book, id)?;
    let balance = book.balance(account.id)?;
    let writer = AmountWriter::new(book, account.currency, locale)?;
    let stated = match book.stated_balance(account.id)? {
        Some(stated) => {
            let held = book.balance_on(account.id, stated.on)?;
            let (balance, owes) = writer.balance(stated.amount, account.kind);
            let (held_shown, held_owes) = writer.balance(held, account.kind);
            Some(Statement {
                balance,
                owes,
                date: locale.short_date(stated.on),
                held: held_shown,
                held_owes,
                matches: held == stated.amount,
            })
        }
        None => None,
    };
    Ok(AccountDetails {
        card: card(&account, balance, &writer),
        currency: account.currency.to_string(),
        closed: account.closed,
        debt: account.kind == AccountKind::Liability,
        can_close: balance.is_zero(),
        stated,
    })
}

/// Adds an account. If `new` says how much is in it, the account gets a
/// starting balance dated on the day the user named, booked against the
/// internal account for starting balances, so the account holds exactly the
/// amount stated on that day (D-111), and the account keeps that statement
/// (D-117). The account and its starting balance are added together or not
/// at all (D-032).
pub fn add_account(
    book: &mut Book,
    new: &NewAccount<'_>,
    locale: &impl Locale,
) -> Result<AccountId, AccountsError> {
    let currency = CurrencyCode::new(new.currency)
        .ok()
        .filter(|currency| currency.iso_minor_digits().is_some())
        .ok_or(AccountsError::UnknownCurrency)?;
    let day = read_day(new.date, locale)?;
    // A currency the book hasn't used has the decimal places ISO 4217 gives
    // it, as soon as the account uses it (D-027).
    let minor_digits = match book
        .currencies()?
        .into_iter()
        .find(|used| used.code() == currency)
    {
        Some(used) => used.minor_digits(),
        None => currency
            .iso_minor_digits()
            .ok_or(AccountsError::UnknownCurrency)?,
    };
    let stated = !new.balance.trim().is_empty();
    let amount = if stated {
        read_amount(new.balance, currency, minor_digits, locale)?
    } else {
        Money::zero(currency)
    };
    let amount = match new.account_type.kind() {
        AccountKind::Liability if amount.is_negative() => return Err(AccountsError::NegativeDebt),
        // What is owed is money that went out of the account.
        AccountKind::Liability => amount
            .checked_neg()
            .map_err(|_| AccountsError::Amount(AmountInputError::TooLarge, currency))?,
        _ => amount,
    };

    let id = AccountId::generate();
    book.as_one_action(Action::AddAccount, |book| -> Result<_, AccountsError> {
        book.use_currency(currency)?;
        book.add_account(&Account {
            id,
            name: new.name.trim().to_owned(),
            kind: new.account_type.kind(),
            currency,
            account_type: Some(new.account_type),
            closed: false,
            parent: None,
        })?;
        if !amount.is_zero() {
            let equity = starting_balances_account(book, currency)?;
            let balancing = amount
                .checked_neg()
                .map_err(|_| AccountsError::Amount(AmountInputError::TooLarge, currency))?;
            let transaction = Transaction::new(
                TransactionId::generate(),
                day,
                None,
                vec![Split::new(id, amount), Split::new(equity, balancing)],
            )
            .map_err(AccountsError::Transaction)?;
            book.add_transaction(&transaction)?;
        }
        if stated {
            book.set_stated_balance(id, Some(&StatedBalance { amount, on: day }))?;
        }
        Ok(id)
    })
}

/// Gives an account a new name.
pub fn rename_account(book: &mut Book, id: &str, name: &str) -> Result<(), AccountsError> {
    let account = find(book, id)?;
    Ok(book.rename_account(account.id, name.trim())?)
}

/// Closes an account. Nothing may be left in it (D-081).
pub fn close_account(book: &mut Book, id: &str) -> Result<(), AccountsError> {
    let account = find(book, id)?;
    Ok(book.close_account(account.id)?)
}

/// Opens a closed account again.
pub fn reopen_account(book: &mut Book, id: &str) -> Result<(), AccountsError> {
    let account = find(book, id)?;
    Ok(book.reopen_account(account.id)?)
}

/// Holds the balance the bank reports on `date`, typed as `bank`, against
/// what KassenSturz has for the account at the end of that day, and works
/// out where a difference can come from (D-029, D-111).
///
/// It changes nothing: a starting balance that looks wrong is for the user to
/// decide on.
pub fn check_against_bank(
    book: &Book,
    id: &str,
    bank: &str,
    date: &str,
    locale: &impl Locale,
) -> Result<BankCheck, AccountsError> {
    let account = find(book, id)?;
    let day = read_day(date, locale)?;
    let minor_digits = book.currency_minor_digits(account.currency)?;
    let bank = read_amount(bank, account.currency, minor_digits, locale)?;
    let bank = match account.kind {
        AccountKind::Liability if bank.is_negative() => return Err(AccountsError::NegativeDebt),
        // What is owed is money that went out of the account.
        AccountKind::Liability => bank
            .checked_neg()
            .map_err(|_| AccountsError::Amount(AmountInputError::TooLarge, account.currency))?,
        _ => bank,
    };
    let held = book.balance_on(account.id, day)?;
    let difference = bank
        .checked_sub(held)
        .map_err(|_| StoreError::AmountOverflow)?;

    let writer = AmountWriter::new(book, account.currency, locale)?;
    let (shown_bank, bank_owes) = writer.balance(bank, account.kind);
    let (shown_book, book_owes) = writer.balance(held, account.kind);
    let (explanations, correction) = if difference.is_zero() {
        (Vec::new(), None)
    } else {
        let (starting, entries) = parts_by_origin(book, &account)?;
        let explanations = explain(
            &account, day, difference, &starting, &entries, &writer, locale,
        )?;
        let correction = match correctable(&starting, day) {
            Err(()) => None,
            Ok(target) => {
                let current = Money::checked_sum(
                    account.currency,
                    starting
                        .iter()
                        .filter(|part| part.bank_date() <= day)
                        .map(|part| part.amount),
                )
                .map_err(|_| StoreError::AmountOverflow)?;
                let would_be = current
                    .checked_add(difference)
                    .map_err(|_| StoreError::AmountOverflow)?;
                let (shown, owes) = writer.balance(would_be, account.kind);
                Some(Correction {
                    would_be: shown,
                    owes,
                    adds: target.is_none(),
                    date: locale.short_date(target.map_or(day, AccountPart::bank_date)),
                })
            }
        };
        (explanations, correction)
    };
    Ok(BankCheck {
        date: locale.short_date(day),
        bank: shown_bank,
        bank_owes,
        book: shown_book,
        book_owes,
        debt: account.kind == AccountKind::Liability,
        matches: difference.is_zero(),
        difference: writer.text(absolute(difference)?),
        book_higher: difference.is_negative(),
        explanations,
        correction,
    })
}

/// Corrects the starting balance so that the account holds on `date` what
/// the bank says, typed as `bank` (D-111): the user confirmed the offer of
/// [`check_against_bank`]. The latest starting balance dated on or before
/// the day changes by the difference; an account without one gets one dated
/// that day. Every other entry stays as it is, and the account keeps what
/// the bank said and the day as its stated balance (D-117); all of it is one
/// change. A starting balance dated after the day is refused, since it would
/// count twice.
pub fn correct_starting_balance(
    book: &mut Book,
    id: &str,
    bank: &str,
    date: &str,
    locale: &impl Locale,
) -> Result<(), AccountsError> {
    let account = find(book, id)?;
    let day = read_day(date, locale)?;
    let minor_digits = book.currency_minor_digits(account.currency)?;
    let bank = read_amount(bank, account.currency, minor_digits, locale)?;
    let bank = match account.kind {
        AccountKind::Liability if bank.is_negative() => return Err(AccountsError::NegativeDebt),
        AccountKind::Liability => bank
            .checked_neg()
            .map_err(|_| AccountsError::Amount(AmountInputError::TooLarge, account.currency))?,
        _ => bank,
    };
    let held = book.balance_on(account.id, day)?;
    let difference = bank
        .checked_sub(held)
        .map_err(|_| StoreError::AmountOverflow)?;
    if difference.is_zero() {
        return Ok(());
    }
    let (starting, _) = parts_by_origin(book, &account)?;
    let target = correctable(&starting, day).map_err(|()| AccountsError::StartingBalanceLater)?;
    let too_large = || AccountsError::Amount(AmountInputError::TooLarge, account.currency);
    let stated = StatedBalance {
        amount: bank,
        on: day,
    };
    match target {
        Some(part) => {
            let transaction = book.transaction(part.transaction)?;
            let equity = starting_balances_account_of(book, &transaction)?;
            let mut splits = transaction.splits().to_vec();
            for split in &mut splits {
                if split.id == part.split {
                    split.amount = split
                        .amount
                        .checked_add(difference)
                        .map_err(|_| too_large())?;
                } else if split.account == Some(equity) {
                    split.amount = split
                        .amount
                        .checked_sub(difference)
                        .map_err(|_| too_large())?;
                }
            }
            let changed = Transaction::new(
                transaction.id(),
                transaction.booking_date(),
                transaction.payee().map(ToOwned::to_owned),
                splits,
            )
            .map_err(AccountsError::Transaction)?;
            book.as_one_action(Action::CorrectStartingBalance, |book| {
                book.update_transaction(&changed)?;
                book.set_stated_balance(account.id, Some(&stated))
            })?;
        }
        None => {
            let balancing = difference.checked_neg().map_err(|_| too_large())?;
            book.as_one_action(
                Action::CorrectStartingBalance,
                |book| -> Result<(), AccountsError> {
                    let equity = starting_balances_account(book, account.currency)?;
                    let transaction = Transaction::new(
                        TransactionId::generate(),
                        day,
                        None,
                        vec![
                            Split::new(account.id, difference),
                            Split::new(equity, balancing),
                        ],
                    )
                    .map_err(AccountsError::Transaction)?;
                    book.add_transaction(&transaction)?;
                    book.set_stated_balance(account.id, Some(&stated))?;
                    Ok(())
                },
            )?;
        }
    }
    Ok(())
}

/// The internal account a starting-balance `transaction` is booked against.
fn starting_balances_account_of(
    book: &Book,
    transaction: &Transaction,
) -> Result<AccountId, AccountsError> {
    let equity: HashSet<AccountId> = book
        .all_accounts()?
        .into_iter()
        .filter(|other| other.kind == AccountKind::Equity)
        .map(|other| other.id)
        .collect();
    transaction
        .splits()
        .iter()
        .find_map(|split| split.account.filter(|account| equity.contains(account)))
        .ok_or(AccountsError::NoSuchEntry)
}

/// Where a `difference` (the bank's balance minus KassenSturz's) can come
/// from, going by what the account holds: only exact facts, no guessing.
fn explain(
    account: &Account,
    day: Date,
    difference: Money,
    starting: &[AccountPart],
    entries: &[(AccountPart, String)],
    writer: &AmountWriter<'_, impl Locale>,
    locale: &impl Locale,
) -> Result<Vec<ErrorText>, AccountsError> {
    let debt = account.kind == AccountKind::Liability;
    let owes = |amount: Money| {
        if debt && amount.is_negative() {
            "true"
        } else {
            "false"
        }
    };
    // An amount without a sign: a debt is written by what is owed.
    let plain = |amount: Money| -> Result<String, AccountsError> {
        Ok(writer.text(absolute(amount)?).exact)
    };

    let mut explanations = Vec::new();
    let later = entries
        .iter()
        .filter(|(part, _)| part.bank_date() > day)
        .count();
    if later > 0 {
        explanations.push(ErrorText {
            code: "bank-check.later-entries",
            params: vec![
                ("count", later.to_string()),
                ("date", locale.short_date(day)),
            ],
        });
    }
    if let Some(part) = starting.iter().find(|part| part.bank_date() > day) {
        explanations.push(ErrorText {
            code: "bank-check.starting-balance-later",
            params: vec![
                ("date", locale.short_date(part.bank_date())),
                ("amount", plain(part.amount)?),
                ("owes", owes(part.amount).into()),
            ],
        });
    }

    // One entry as large as the difference, or half of it, is the usual
    // slip: entered twice, or the wrong way round. The latest one counts.
    let counted = || entries.iter().filter(|(part, _)| part.bank_date() <= day);
    let entry_text = |code, part: &AccountPart, payee: &String| {
        Ok::<_, AccountsError>(ErrorText {
            code,
            params: vec![
                ("date", locale.short_date(part.bank_date())),
                ("amount", plain(part.amount)?),
                ("other_party", payee.clone()),
            ],
        })
    };
    let minus = difference
        .checked_neg()
        .map_err(|_| StoreError::AmountOverflow)?;
    if let Some((part, payee)) = counted()
        .filter(|(part, _)| part.amount == minus)
        .max_by_key(|(part, _)| part.bank_date())
    {
        explanations.push(entry_text("bank-check.entry-twice", part, payee)?);
    }
    if difference.minor_units() % 2 == 0
        && let Some((part, payee)) = counted()
            .filter(|(part, _)| part.amount.minor_units() == minus.minor_units() / 2)
            .max_by_key(|(part, _)| part.bank_date())
    {
        explanations.push(entry_text("bank-check.entry-wrong-way", part, payee)?);
    }

    explanations.push(ErrorText {
        code: "bank-check.entry-missing",
        params: vec![
            (
                "direction",
                if difference.is_negative() {
                    "out"
                } else {
                    "in"
                }
                .into(),
            ),
            ("amount", plain(difference)?),
        ],
    });

    // What the starting balance would have to be for both to agree: the
    // user decides whether that is the mistake; it is never changed here.
    let counted_start = starting.iter().filter(|part| part.bank_date() <= day);
    let current = Money::checked_sum(account.currency, counted_start.map(|part| part.amount))
        .map_err(|_| StoreError::AmountOverflow)?;
    let would_be = current
        .checked_add(difference)
        .map_err(|_| StoreError::AmountOverflow)?;
    let (shown, would_owe) = writer.balance(would_be, account.kind);
    let mut params = vec![("would_be", shown.exact), ("owes", would_owe.to_string())];
    let code = if starting.iter().any(|part| part.bank_date() <= day) {
        let (current, current_owes) = writer.balance(current, account.kind);
        params.push(("current", current.exact));
        params.push(("current_owes", current_owes.to_string()));
        "bank-check.starting-balance"
    } else {
        "bank-check.no-starting-balance"
    };
    explanations.push(ErrorText { code, params });
    Ok(explanations)
}

/// `amount` without its sign.
fn absolute(amount: Money) -> Result<Money, StoreError> {
    if amount.is_negative() {
        amount.checked_neg().map_err(|_| StoreError::AmountOverflow)
    } else {
        Ok(amount)
    }
}

/// The internal account that starting balances in `currency` are booked
/// against (`docs/GLOSSARY.md`), made when there is none yet.
fn starting_balances_account(
    book: &mut Book,
    currency: CurrencyCode,
) -> Result<AccountId, StoreError> {
    let found = book
        .all_accounts()?
        .into_iter()
        .find(|account| account.kind == AccountKind::Equity && account.currency == currency);
    if let Some(account) = found {
        return Ok(account.id);
    }
    // Names are never shared, so a second currency's account gets its code.
    let others = book
        .all_accounts()?
        .iter()
        .any(|account| account.kind == AccountKind::Equity);
    let id = AccountId::generate();
    book.add_account(&Account {
        id,
        // Never shown (`AccountKind::Equity`).
        name: if others {
            format!("Starting balances ({currency})")
        } else {
            "Starting balances".into()
        },
        kind: AccountKind::Equity,
        currency,
        account_type: None,
        closed: false,
        parent: None,
    })?;
    Ok(id)
}

/// The account (not category) the UI means by `id`.
fn find(book: &Book, id: &str) -> Result<Account, AccountsError> {
    book.all_accounts()?
        .into_iter()
        .find(|account| account.kind.is_account() && account.id.to_string() == id)
        .ok_or(AccountsError::NoSuchEntry)
}

/// The day the user typed: today if nothing, and never one that hasn't come.
fn read_day(text: &str, locale: &impl Locale) -> Result<Date, AccountsError> {
    let day = if text.trim().is_empty() {
        locale.today()
    } else {
        locale.read_date(text.trim()).ok_or(AccountsError::Date)?
    };
    if day > locale.today() {
        return Err(AccountsError::DateInFuture);
    }
    Ok(day)
}

fn read_amount(
    text: &str,
    currency: CurrencyCode,
    minor_digits: u8,
    locale: &impl Locale,
) -> Result<Money, AccountsError> {
    let format = locale.amount_format(currency);
    let group_separator = locale.group_separator();
    let reader = AmountReader {
        format: &format,
        group_separator: &group_separator,
        currency,
        minor_digits,
    };
    reader
        .read(text)
        .map_err(|error| AccountsError::Amount(error, currency))
}

fn card(account: &Account, balance: Money, writer: &AmountWriter<'_, impl Locale>) -> AccountCard {
    let (shown, owes) = writer.balance(balance, account.kind);
    AccountCard {
        id: account.id.to_string(),
        name: account.name.clone(),
        account_type: account.account_type.map_or("", type_key),
        balance: shown,
        owes,
    }
}

/// `error` as the UI shows it, with every parameter as text.
pub fn error_text(book: &Book, error: &AccountsError, locale: &impl Locale) -> ErrorText {
    match error {
        AccountsError::Amount(error, currency) => {
            ErrorText::amount(*error, book, *currency, locale)
        }
        AccountsError::NegativeDebt => ErrorText::plain("input.negative-debt"),
        AccountsError::Date => ErrorText {
            code: "input.malformed-date",
            params: vec![("example", locale.short_date(locale.today()))],
        },
        AccountsError::DateInFuture => ErrorText::plain("input.date-in-future"),
        AccountsError::UnknownCurrency => ErrorText::plain("input.unknown-currency"),
        AccountsError::NoSuchEntry => ErrorText::plain("input.no-such-entry"),
        AccountsError::StartingBalanceLater => ErrorText::plain("bank-check.cannot-correct"),
        AccountsError::Store(error) => report_text(book, &error.report(), locale),
        AccountsError::Transaction(error) => report_text(book, &error.report(), locale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::first_run::FirstRun;
    use crate::first_start::tests::started;
    use crate::number_format::{DecimalPlaces, NumberFormat, UserLocale};
    use crate::register::tests::TestLocale;
    use crate::register::{Sort, entries, register_view};

    const EUR: CurrencyCode = match CurrencyCode::new("EUR") {
        Ok(code) => code,
        Err(_) => panic!("EUR is a valid code"),
    };

    fn book() -> Book {
        Book::create_in_memory().expect("book in memory")
    }

    fn new<'a>(
        name: &'a str,
        account_type: AccountType,
        currency: &'a str,
        balance: &'a str,
        date: &'a str,
    ) -> NewAccount<'a> {
        NewAccount {
            name,
            account_type,
            currency,
            balance,
            date,
        }
    }

    /// Adds a checking account in euros with `balance` on 1 October.
    fn checking(book: &mut Book, name: &str, balance: &str) -> String {
        let id = add(
            book,
            &new(name, AccountType::Checking, "EUR", balance, "2026-10-01"),
        );
        id.to_string()
    }

    fn add(book: &mut Book, account: &NewAccount<'_>) -> AccountId {
        add_account(book, account, &TestLocale).expect("account added")
    }

    fn id_of(book: &Book, name: &str) -> String {
        book.accounts(true)
            .expect("accounts")
            .into_iter()
            .find(|account| account.name == name)
            .expect("account there")
            .id
            .to_string()
    }

    /// A payment from or to an account, in euros.
    fn pay(book: &mut Book, account: &str, day: &str, minor_units: i64, payee: &str) {
        let id = book
            .all_accounts()
            .expect("accounts")
            .into_iter()
            .find(|other| other.id.to_string() == account)
            .expect("account")
            .id;
        let spending = Account {
            id: AccountId::generate(),
            name: format!("Spending {day} {payee}"),
            kind: AccountKind::Expense,
            currency: EUR,
            account_type: None,
            closed: false,
            parent: None,
        };
        book.add_account(&spending).expect("category added");
        let amount = Money::from_minor_units(minor_units, EUR);
        let transaction = Transaction::new(
            TransactionId::generate(),
            day.parse().expect("day"),
            Some(payee.into()),
            vec![
                Split::new(id, amount),
                Split::new(spending.id, amount.checked_neg().expect("negated")),
            ],
        )
        .expect("balanced");
        book.add_transaction(&transaction).expect("added");
    }

    fn code(book: &Book, error: &AccountsError) -> &'static str {
        error_text(book, error, &TestLocale).code
    }

    fn explanation_codes(check: &BankCheck) -> Vec<&'static str> {
        check.explanations.iter().map(|text| text.code).collect()
    }

    fn param<'a>(text: &'a ErrorText, name: &str) -> &'a str {
        text.params
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
            .unwrap_or_else(|| panic!("parameter {name} of {}", text.code))
    }

    #[test]
    fn a_new_account_holds_the_stated_amount_on_the_stated_day() {
        let mut book = book();
        let id = checking(&mut book, "Checking account", "1.250,50");

        let account = book.accounts(false).expect("accounts").remove(0);
        assert_eq!(account.id.to_string(), id);
        let day = |day: &str| day.parse().expect("day");
        assert_eq!(
            book.balance_on(account.id, day("2026-09-30"))
                .expect("balance"),
            Money::zero(EUR)
        );
        assert_eq!(
            book.balance_on(account.id, day("2026-10-01"))
                .expect("balance"),
            Money::from_minor_units(125_050, EUR)
        );
        // The register shows the amount as the starting balance, on that day.
        let rows = entries(&book, account.id).expect("entries");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].starting_balance());
        assert_eq!(rows[0].date, day("2026-10-01"));
        let shown =
            register_view(&book, account.id, Sort::default(), &TestLocale).expect("register");
        assert_eq!(shown.balance.exact, "1250,50 €");
        // The account keeps what the user said the bank showed (D-117), and
        // the account page sees that it still holds it.
        assert_eq!(
            book.stated_balance(account.id).expect("statement"),
            Some(StatedBalance {
                amount: Money::from_minor_units(125_050, EUR),
                on: day("2026-10-01"),
            })
        );
        let details = details(&book, &id, &TestLocale).expect("details");
        assert_eq!(
            details.stated,
            Some(Statement {
                balance: shown.balance.clone(),
                owes: false,
                date: "2026-10-01".into(),
                held: shown.balance,
                held_owes: false,
                matches: true,
            })
        );
    }

    #[test]
    fn nothing_typed_is_an_empty_account_dated_today() {
        let mut book = book();
        add(
            &mut book,
            &new("Wallet", AccountType::Cash, "EUR", "  ", ""),
        );
        let view = view(&book, &TestLocale).expect("view");
        assert_eq!(view.sections[0].accounts[0].balance.exact, "0,00 €");
        // No starting balance was booked, so there is no internal account.
        assert!(
            book.all_accounts()
                .expect("accounts")
                .iter()
                .all(|account| account.kind != AccountKind::Equity)
        );
        // Nothing was stated, so nothing is kept or checked.
        let id = id_of(&book, "Wallet");
        assert_eq!(
            details(&book, &id, &TestLocale).expect("details").stated,
            None
        );
    }

    #[test]
    fn the_account_page_notices_when_the_stated_balance_no_longer_holds() {
        // Told 1.000 on 1 October. An entry from before that day, e.g. one
        // the user adds from an old statement, changes what the account
        // held then: the account page says so, and the stated balance is
        // what the check is prefilled with (D-117).
        let mut book = book();
        let id = checking(&mut book, "Checking account", "1.000,00");
        pay(&mut book, &id, "2026-09-20", -2500, "Bakery");
        let page = details(&book, &id, &TestLocale).expect("details");
        let stated = page.stated.expect("stated");
        assert_eq!(
            (
                stated.balance.exact.as_str(),
                stated.date.as_str(),
                stated.held.exact.as_str(),
                stated.matches
            ),
            ("1000,00 €", "2026-10-01", "975,00 €", false)
        );

        // The check reads the stated text back and offers the correction;
        // confirmed, the account holds what was stated again, and the
        // statement itself is unchanged.
        let check =
            check_against_bank(&book, &id, &stated.balance.exact, &stated.date, &TestLocale)
                .expect("checked");
        assert!(!check.matches);
        let offer = check.correction.as_ref().expect("a correction offered");
        assert_eq!(offer.would_be.exact, "1025,00 €");
        correct_starting_balance(
            &mut book,
            &id,
            &stated.balance.exact,
            &stated.date,
            &TestLocale,
        )
        .expect("corrected");
        let again = details(&book, &id, &TestLocale)
            .expect("details")
            .stated
            .expect("stated");
        assert!(again.matches);
        assert_eq!(
            (again.balance.exact.as_str(), again.date.as_str()),
            ("1000,00 €", "2026-10-01")
        );
        assert_eq!(
            starting_balances(&book, &id),
            [("2026-10-01".to_owned(), 102_500)]
        );
        // Undo takes the correction back, and the page notices again.
        book.undo().expect("undone");
        assert!(
            !details(&book, &id, &TestLocale)
                .expect("details")
                .stated
                .expect("stated")
                .matches
        );
    }

    #[test]
    fn a_credit_card_is_what_the_user_owes() {
        let mut book = book();
        checking(&mut book, "Checking account", "1.000,00");
        add(
            &mut book,
            &new(
                "Card",
                AccountType::CreditCard,
                "EUR",
                "300,00",
                "2026-10-02",
            ),
        );
        let view = view(&book, &TestLocale).expect("view");
        let card = &view.sections[0].accounts[1];
        assert_eq!(card.account_type, "credit-card");
        assert_eq!((card.balance.exact.as_str(), card.owes), ("300,00 €", true));
        // All together: what the user has minus what they owe.
        let together = &view.sections[0];
        assert_eq!(
            (together.together.exact.as_str(), together.together_owes),
            ("700,00 €", false)
        );
    }

    #[test]
    fn amounts_of_different_currencies_are_never_added_up() {
        let mut book = book();
        checking(&mut book, "Checking account", "1.000,00");
        add(
            &mut book,
            &new(
                "Dollars",
                AccountType::Savings,
                "USD",
                "50,00",
                "2026-10-01",
            ),
        );
        add(
            &mut book,
            &new("Yen", AccountType::Cash, "JPY", "1500", "2026-10-01"),
        );
        let view = view(&book, &TestLocale).expect("view");
        let currencies: Vec<&str> = view
            .sections
            .iter()
            .map(|section| section.currency.as_str())
            .collect();
        // The currency used first comes first.
        assert_eq!(currencies, ["EUR", "USD", "JPY"]);
        let amounts: Vec<String> = view
            .sections
            .iter()
            .map(|section| section.together.exact.clone())
            .collect();
        assert_eq!(amounts[0], "1000,00 €");
        // The yen has no decimal places, the dollar two, as ISO 4217 says.
        assert_eq!(
            book.balance(book.accounts(false).expect("accounts")[2].id)
                .expect("balance")
                .minor_units(),
            1500
        );
        assert_eq!(
            book.currency_minor_digits(CurrencyCode::new("USD").expect("code"))
                .expect("digits"),
            2
        );
        // Each currency has one internal account for its starting balances.
        let equity = book
            .all_accounts()
            .expect("accounts")
            .into_iter()
            .filter(|account| account.kind == AccountKind::Equity)
            .count();
        assert_eq!(equity, 3);
    }

    #[test]
    fn totals_add_up_the_exact_amounts_before_rounding() {
        let mut book = book();
        let locale = UserLocale::new(
            TestLocale,
            NumberFormat {
                decimal_places: DecimalPlaces::None,
                ..NumberFormat::default()
            },
        );
        for name in ["Cash", "Wallet", "Tin"] {
            add_account(
                &mut book,
                &new(name, AccountType::Cash, "EUR", "0,40", ""),
                &locale,
            )
            .expect("account added");
        }
        let view = view(&book, &locale).expect("view");
        // Each 0,40 € shows as 0 €; together they are 1,20 €, shown as 1 €.
        assert!(
            view.sections[0]
                .accounts
                .iter()
                .all(|card| card.balance.text == "0 €")
        );
        assert_eq!(view.sections[0].together.text, "1 €");
        assert_eq!(view.sections[0].together.exact, "1,20 €");
    }

    #[test]
    fn offers_the_currencies_in_use_first() {
        let mut book = book();
        let usd = CurrencyCode::new("USD").expect("code");
        let choices = currency_choices(&book, usd).expect("choices");
        assert_eq!(choices[0], usd);
        assert_eq!(choices.iter().filter(|code| **code == usd).count(), 1);
        assert!(choices.contains(&EUR));

        checking(&mut book, "Checking account", "");
        let choices = currency_choices(&book, usd).expect("choices");
        assert_eq!(choices[..2], [EUR, CurrencyCode::new("AED").expect("code")]);
        assert_eq!(usual_currency(&book, usd).expect("usual"), EUR);
        assert_eq!(
            usual_currency(&Book::create_in_memory().expect("book"), usd).expect("usual"),
            usd
        );
    }

    #[test]
    fn explains_what_it_refuses() {
        let mut book = book();
        checking(&mut book, "Checking account", "100,00");
        let mut refused = |account: NewAccount<'_>| {
            let error = add_account(&mut book, &account, &TestLocale).expect_err("refused");
            (code(&book, &error), error_text(&book, &error, &TestLocale))
        };
        let (found, text) = refused(new("A", AccountType::Cash, "EUR", "12.50.0", ""));
        assert_eq!(found, "input.malformed-amount");
        assert_eq!(param(&text, "example"), "1234,56 €".trim_end_matches(" €"));
        assert_eq!(
            refused(new("A", AccountType::Cash, "EUR", "1,234", "")).0,
            "input.too-many-decimal-places"
        );
        assert_eq!(
            refused(new("A", AccountType::CreditCard, "EUR", "-5", "")).0,
            "input.negative-debt"
        );
        assert_eq!(
            refused(new("A", AccountType::Cash, "EUR", "5", "yesterday")).0,
            "input.malformed-date"
        );
        // Today is 19 October 2026 for the tests.
        assert_eq!(
            refused(new("A", AccountType::Cash, "EUR", "5", "2026-10-20")).0,
            "input.date-in-future"
        );
        assert_eq!(
            refused(new("A", AccountType::Cash, "XXX", "5", "")).0,
            "input.unknown-currency"
        );
        assert_eq!(
            refused(new("A", AccountType::Cash, "DEM", "5", "")).0,
            "input.unknown-currency"
        );
        assert_eq!(
            refused(new("  ", AccountType::Cash, "EUR", "5", "")).0,
            "store.empty-account-name"
        );
        let (found, text) = refused(new("checking ACCOUNT", AccountType::Cash, "EUR", "5", ""));
        assert_eq!(found, "store.duplicate-account-name");
        assert_eq!(param(&text, "existing"), "Checking account");
    }

    #[test]
    fn a_refused_account_leaves_nothing_behind() {
        let mut book = book();
        checking(&mut book, "Checking account", "100,00");
        let before = (
            book.all_accounts().expect("accounts"),
            book.currencies().expect("currencies"),
            book.balance(book.accounts(false).expect("accounts")[0].id)
                .expect("balance"),
        );
        // The name is taken, after the starting balance was read: nothing of
        // the new account may stay, not even its currency.
        add_account(
            &mut book,
            &new("Checking account", AccountType::Savings, "USD", "75,00", ""),
            &TestLocale,
        )
        .expect_err("refused");
        let after = (
            book.all_accounts().expect("accounts"),
            book.currencies().expect("currencies"),
            book.balance(book.accounts(false).expect("accounts")[0].id)
                .expect("balance"),
        );
        assert_eq!(before, after);
    }

    #[test]
    fn renames_closes_and_opens_accounts() {
        let mut book = book();
        let checking = checking(&mut book, "Checking account", "100,00");
        let wallet = id_of(&book, "Checking account");
        assert_eq!(wallet, checking);
        let cash = add(&mut book, &new("Wallet", AccountType::Cash, "EUR", "", "")).to_string();

        rename_account(&mut book, &cash, "  Purse ").expect("renamed");
        assert_eq!(
            details(&book, &cash, &TestLocale)
                .expect("details")
                .card
                .name,
            "Purse"
        );
        let error = rename_account(&mut book, &cash, "CHECKING account").expect_err("taken");
        assert_eq!(code(&book, &error), "store.duplicate-account-name");
        let error = rename_account(&mut book, &cash, " ").expect_err("empty");
        assert_eq!(code(&book, &error), "store.empty-account-name");

        // Money is left in the checking account, so it stays open.
        let error = close_account(&mut book, &checking).expect_err("not empty");
        let text = error_text(&book, &error, &TestLocale);
        assert_eq!(text.code, "store.account-not-empty");
        assert_eq!(param(&text, "account"), "Checking account");
        assert_eq!(param(&text, "balance"), "100,00 €");
        assert!(
            !details(&book, &checking, &TestLocale)
                .expect("details")
                .can_close
        );

        // The empty one closes, drops out of the list and can be opened again.
        assert!(
            details(&book, &cash, &TestLocale)
                .expect("details")
                .can_close
        );
        close_account(&mut book, &cash).expect("closed");
        let listed = view(&book, &TestLocale).expect("view");
        assert_eq!(listed.sections[0].accounts.len(), 1);
        assert_eq!(listed.closed[0].name, "Purse");
        assert!(details(&book, &cash, &TestLocale).expect("details").closed);
        reopen_account(&mut book, &cash).expect("opened");
        let listed = view(&book, &TestLocale).expect("view");
        assert_eq!(
            (listed.sections[0].accounts.len(), listed.closed.len()),
            (2, 0)
        );
    }

    #[test]
    fn only_accounts_are_found_by_their_id() {
        let mut book = book();
        checking(&mut book, "Checking account", "");
        let category = Account {
            id: AccountId::generate(),
            name: "Groceries".into(),
            kind: AccountKind::Expense,
            currency: EUR,
            account_type: None,
            closed: false,
            parent: None,
        };
        book.use_currency(EUR).expect("currency");
        book.add_account(&category).expect("category");
        for id in [
            category.id.to_string(),
            "nonsense".to_owned(),
            String::new(),
        ] {
            let error = close_account(&mut book, &id).expect_err("no account");
            assert_eq!(code(&book, &error), "input.no-such-entry");
            assert!(details(&book, &id, &TestLocale).is_err());
        }
    }

    #[test]
    fn the_first_run_mock_sees_the_accounts_made_by_this_screen() {
        let mut book = started();
        checking(&mut book, "Checking account", "10,00");
        checking(&mut book, "Wallet", "20,00");
        // The mock lists both, in the order they were added, and finds the
        // default categories the first start wrote (D-116).
        let mock = FirstRun::load(&book, usual_currency(&book, EUR).expect("usual")).expect("mock");
        assert_eq!(
            mock.account_names(&book).expect("names"),
            ["Checking account", "Wallet"]
        );
        assert_eq!(mock.category_names(true), ["Salary", "Other income"]);
        let listed = view(&book, &TestLocale).expect("view");
        assert_eq!(listed.sections[0].together.exact, "30,00 €");
        // One internal account serves both starting balances.
        assert_eq!(
            book.all_accounts()
                .expect("accounts")
                .iter()
                .filter(|account| account.kind == AccountKind::Equity)
                .count(),
            1
        );
    }

    #[test]
    fn agrees_with_the_bank() {
        let mut book = book();
        let id = checking(&mut book, "Checking account", "1.000,00");
        pay(&mut book, &id, "2026-10-05", -2500, "Bakery");
        let check =
            check_against_bank(&book, &id, "975,00", "2026-10-05", &TestLocale).expect("checked");
        assert!(check.matches && check.explanations.is_empty());
        assert_eq!(check.book.exact, "975,00 €");
        assert_eq!(check.difference.exact, "0,00 €");
        // Nothing typed as the day means today.
        let today = check_against_bank(&book, &id, "975,00", " ", &TestLocale).expect("checked");
        assert_eq!(today.date, "2026-10-19");
        assert!(today.matches);
    }

    #[test]
    fn explains_a_difference_that_is_a_missing_history() {
        // The bank holds 800 where KassenSturz, told 1.000 at the start,
        // has 1.000: the history before the starting balance isn't there.
        let mut book = book();
        let id = checking(&mut book, "Checking account", "1.000,00");
        let check =
            check_against_bank(&book, &id, "800,00", "2026-10-10", &TestLocale).expect("checked");
        assert!(!check.matches);
        assert!(check.book_higher);
        assert_eq!(check.difference.exact, "200,00 €");
        assert_eq!(
            explanation_codes(&check),
            ["bank-check.entry-missing", "bank-check.starting-balance"]
        );
        let missing = &check.explanations[0];
        assert_eq!(
            (param(missing, "direction"), param(missing, "amount")),
            ("out", "200,00 €")
        );
        // What the starting balance would have to be, for the user to judge;
        // nothing was changed.
        let start = &check.explanations[1];
        assert_eq!(
            (param(start, "current"), param(start, "would_be")),
            ("1000,00 €", "800,00 €")
        );
        let account = book.accounts(false).expect("accounts").remove(0);
        assert_eq!(
            book.balance(account.id).expect("balance"),
            Money::from_minor_units(100_000, EUR)
        );
    }

    #[test]
    fn recognizes_an_entry_made_twice_and_one_made_the_wrong_way_round() {
        let mut book = book();
        let id = checking(&mut book, "Checking account", "1.000,00");
        pay(&mut book, &id, "2026-10-04", -2500, "Bakery");
        pay(&mut book, &id, "2026-10-05", -2500, "Bakery");
        pay(&mut book, &id, "2026-10-06", 1000, "Refund");

        // The bank has the bakery once: 25,00 € less in KassenSturz.
        let twice =
            check_against_bank(&book, &id, "985,00", "2026-10-06", &TestLocale).expect("checked");
        assert!(!twice.matches);
        assert_eq!(
            explanation_codes(&twice),
            [
                "bank-check.entry-twice",
                "bank-check.entry-missing",
                "bank-check.starting-balance"
            ]
        );
        // The latest of the entries as large as the difference.
        assert_eq!(param(&twice.explanations[0], "date"), "2026-10-05");
        assert_eq!(param(&twice.explanations[0], "amount"), "25,00 €");
        assert_eq!(param(&twice.explanations[0], "other_party"), "Bakery");

        // The refund of 10,00 € was a payment: the bank is 20,00 € lower.
        let wrong =
            check_against_bank(&book, &id, "940,00", "2026-10-06", &TestLocale).expect("checked");
        assert_eq!(
            explanation_codes(&wrong),
            [
                "bank-check.entry-wrong-way",
                "bank-check.entry-missing",
                "bank-check.starting-balance"
            ]
        );
        assert_eq!(param(&wrong.explanations[0], "amount"), "10,00 €");
        assert_eq!(param(&wrong.explanations[0], "other_party"), "Refund");
    }

    #[test]
    fn says_what_the_day_leaves_out() {
        let mut book = book();
        let id = checking(&mut book, "Checking account", "1.000,00");
        pay(&mut book, &id, "2026-10-12", -4000, "Pharmacy");
        // The starting balance is dated after the day checked, so it isn't
        // counted, and neither is the pharmacy.
        let check =
            check_against_bank(&book, &id, "500,00", "2026-09-20", &TestLocale).expect("checked");
        assert_eq!(check.book.exact, "0,00 €");
        assert_eq!(
            explanation_codes(&check),
            [
                "bank-check.later-entries",
                "bank-check.starting-balance-later",
                "bank-check.entry-missing",
                "bank-check.no-starting-balance"
            ]
        );
        assert_eq!(param(&check.explanations[0], "count"), "1");
        assert_eq!(param(&check.explanations[1], "date"), "2026-10-01");
        assert_eq!(param(&check.explanations[1], "amount"), "1000,00 €");
        assert_eq!(param(&check.explanations[3], "would_be"), "500,00 €");
    }

    #[test]
    fn checks_a_credit_card_in_what_is_owed() {
        let mut book = book();
        let card = add(
            &mut book,
            &new(
                "Card",
                AccountType::CreditCard,
                "EUR",
                "300,00",
                "2026-10-01",
            ),
        )
        .to_string();
        let agrees =
            check_against_bank(&book, &card, "300,00", "2026-10-05", &TestLocale).expect("checked");
        assert!(agrees.matches && agrees.debt && agrees.bank_owes && agrees.book_owes);

        // The bank says 280,00 € is owed: KassenSturz has 20,00 € less money,
        // and the hint says what is owed at the start.
        let check =
            check_against_bank(&book, &card, "280,00", "2026-10-05", &TestLocale).expect("checked");
        assert!(!check.book_higher);
        assert_eq!(
            (check.bank.exact.as_str(), check.book.exact.as_str()),
            ("280,00 €", "300,00 €")
        );
        assert_eq!(check.difference.exact, "20,00 €");
        let start = check.explanations.last().expect("explanations");
        assert_eq!(start.code, "bank-check.starting-balance");
        assert_eq!(
            (param(start, "would_be"), param(start, "owes")),
            ("280,00 €", "true")
        );
        let error = check_against_bank(&book, &card, "-5", "", &TestLocale).expect_err("refused");
        assert_eq!(code(&book, &error), "input.negative-debt");
    }

    #[test]
    fn refuses_what_it_cannot_compare() {
        let mut book = book();
        let id = checking(&mut book, "Checking account", "10,00");
        let check = |bank: &str, date: &str| {
            let error =
                check_against_bank(&book, &id, bank, date, &TestLocale).expect_err("refused");
            code(&book, &error)
        };
        assert_eq!(check("", ""), "input.empty-amount");
        assert_eq!(check("ten", ""), "input.malformed-amount");
        assert_eq!(check("10,00", "soon"), "input.malformed-date");
        assert_eq!(check("10,00", "2026-12-24"), "input.date-in-future");
        let error = check_against_bank(&book, "gone", "1", "", &TestLocale).expect_err("refused");
        assert_eq!(code(&book, &error), "input.no-such-entry");
    }

    /// The starting-balance parts of the account, as (day, amount).
    fn starting_balances(book: &Book, account: &str) -> Vec<(String, i64)> {
        let account = find(book, account).expect("account");
        let (starting, _) = parts_by_origin(book, &account).expect("parts");
        starting
            .iter()
            .map(|part| (part.bank_date().to_string(), part.amount.minor_units()))
            .collect()
    }

    #[test]
    fn offers_to_correct_the_starting_balance_and_does_so_on_confirmation() {
        // Told 1.000 at the start, KassenSturz has 1.000 where the bank holds
        // 800: the history before the starting balance isn't there (D-111).
        let mut book = book();
        let id = checking(&mut book, "Checking account", "1.000,00");
        pay(&mut book, &id, "2026-10-05", -2500, "Bakery");
        let check =
            check_against_bank(&book, &id, "775,00", "2026-10-10", &TestLocale).expect("checked");
        let offer = check.correction.as_ref().expect("a correction offered");
        assert_eq!(
            (
                offer.would_be.exact.as_str(),
                offer.owes,
                offer.adds,
                offer.date.as_str()
            ),
            ("800,00 €", false, false, "2026-10-01")
        );
        // Nothing changed by the check itself.
        assert_eq!(
            starting_balances(&book, &id),
            [("2026-10-01".to_owned(), 100_000)]
        );
        let changes = book.history().expect("history").len();

        correct_starting_balance(&mut book, &id, "775,00", "2026-10-10", &TestLocale)
            .expect("corrected");
        assert_eq!(
            starting_balances(&book, &id),
            [("2026-10-01".to_owned(), 80_000)]
        );
        let again =
            check_against_bank(&book, &id, "775,00", "2026-10-10", &TestLocale).expect("checked");
        assert!(again.matches);
        assert!(again.correction.is_none());
        // The other entry stays, the account adds up, and the correction is
        // one change the user can undo.
        assert_eq!(
            book.balance(find(&book, &id).expect("account").id)
                .expect("balance")
                .minor_units(),
            77_500
        );
        let history = book.history().expect("history");
        assert_eq!(history.len(), changes + 1);
        assert_eq!(history[0].action, Action::CorrectStartingBalance);
        // The account now keeps what the bank said on the day checked, and
        // undo takes the correction and the statement back together.
        let account = find(&book, &id).expect("account").id;
        let day = |day: &str| day.parse().expect("day");
        assert_eq!(
            book.stated_balance(account).expect("statement"),
            Some(StatedBalance {
                amount: Money::from_minor_units(77_500, EUR),
                on: day("2026-10-10"),
            })
        );
        book.undo().expect("undone");
        assert_eq!(
            starting_balances(&book, &id),
            [("2026-10-01".to_owned(), 100_000)]
        );
        assert_eq!(
            book.stated_balance(account).expect("statement"),
            Some(StatedBalance {
                amount: Money::from_minor_units(100_000, EUR),
                on: day("2026-10-01"),
            })
        );
    }

    #[test]
    fn a_correction_adds_a_starting_balance_to_an_account_without_one() {
        let mut book = book();
        let id = add(
            &mut book,
            &new("Wallet", AccountType::Cash, "EUR", "", "2026-10-01"),
        )
        .to_string();
        pay(&mut book, &id, "2026-10-05", -2500, "Kiosk");
        let check =
            check_against_bank(&book, &id, "75,00", "2026-10-05", &TestLocale).expect("checked");
        let offer = check.correction.as_ref().expect("a correction offered");
        assert_eq!(
            (
                offer.would_be.exact.as_str(),
                offer.adds,
                offer.date.as_str()
            ),
            ("100,00 €", true, "2026-10-05")
        );
        assert!(starting_balances(&book, &id).is_empty());

        correct_starting_balance(&mut book, &id, "75,00", "2026-10-05", &TestLocale)
            .expect("corrected");
        assert_eq!(
            starting_balances(&book, &id),
            [("2026-10-05".to_owned(), 10_000)]
        );
        assert!(
            check_against_bank(&book, &id, "75,00", "2026-10-05", &TestLocale)
                .expect("checked")
                .matches
        );
        // The register shows it as a starting balance (the seam with the
        // mock's register, M1.M).
        let rows = register_view(
            &book,
            find(&book, &id).expect("account").id,
            Sort::default(),
            &TestLocale,
        )
        .expect("register")
        .rows;
        assert!(
            rows.iter()
                .any(|row| row.starting_balance && row.date == "2026-10-05")
        );
        assert_eq!(
            book.history().expect("history")[0].action,
            Action::CorrectStartingBalance
        );
        // An account added without a balance had no stated balance; the
        // correction gives it one.
        assert_eq!(
            book.stated_balance(find(&book, &id).expect("account").id)
                .expect("statement"),
            Some(StatedBalance {
                amount: Money::from_minor_units(7500, EUR),
                on: "2026-10-05".parse().expect("day"),
            })
        );
    }

    #[test]
    fn a_correction_before_the_starting_balance_is_not_offered() {
        // Checking a day before the starting balance: correcting from there
        // would count the money twice, so it is refused (D-116).
        let mut book = book();
        let id = checking(&mut book, "Checking account", "1.000,00");
        let check =
            check_against_bank(&book, &id, "500,00", "2026-09-20", &TestLocale).expect("checked");
        assert!(!check.matches);
        assert!(check.correction.is_none());
        assert!(explanation_codes(&check).contains(&"bank-check.starting-balance-later"));
        let error = correct_starting_balance(&mut book, &id, "500,00", "2026-09-20", &TestLocale)
            .expect_err("refused");
        assert_eq!(
            error_text(&book, &error, &TestLocale).code,
            "bank-check.cannot-correct"
        );
        assert_eq!(
            starting_balances(&book, &id),
            [("2026-10-01".to_owned(), 100_000)]
        );
        assert_eq!(book.history().expect("history").len(), 1);

        // Agreeing figures leave nothing to correct.
        correct_starting_balance(&mut book, &id, "1.000,00", "2026-10-10", &TestLocale)
            .expect("nothing to do");
        assert_eq!(book.history().expect("history").len(), 1);
    }

    #[test]
    fn corrects_a_credit_card_in_what_is_owed() {
        let mut book = book();
        let id = add(
            &mut book,
            &new(
                "Credit card",
                AccountType::CreditCard,
                "EUR",
                "300",
                "2026-10-01",
            ),
        )
        .to_string();
        let check =
            check_against_bank(&book, &id, "350", "2026-10-10", &TestLocale).expect("checked");
        let offer = check.correction.as_ref().expect("a correction offered");
        assert_eq!(
            (offer.would_be.exact.as_str(), offer.owes),
            ("350,00 €", true)
        );
        correct_starting_balance(&mut book, &id, "350", "2026-10-10", &TestLocale)
            .expect("corrected");
        assert_eq!(
            starting_balances(&book, &id),
            [("2026-10-01".to_owned(), -35_000)]
        );
        let again =
            check_against_bank(&book, &id, "350", "2026-10-10", &TestLocale).expect("checked");
        assert!(again.matches && again.book_owes);
    }
}
