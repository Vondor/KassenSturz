// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The starting balance of an account, worked out from what its bank says
//! (D-111, D-903).
//!
//! A starting balance is a transaction between the account and the internal
//! account for starting balances (`docs/GLOSSARY.md`). The user states what
//! the account holds on a day, usually today; the history a bank or file
//! delivers afterwards mostly lies *before* that day. Left alone, that
//! history would move the stated amount a second time (B-411, #1237). So
//! the starting balance is worked out again from a balance that is known,
//! and dated before the first entry, where it belongs:
//!
//! - [`plan_start`] and [`apply_start`] are the correction the user confirms
//!   in "Check against my bank" (D-116).
//! - [`derive_start_after_import`] does the same inside the first import
//!   into an account, from the bank's own balance (D-903), so the preview
//!   shows it and one Undo takes back the import and the starting balance.
//!
//! Nothing here guesses: every amount is an exact sum of the account's
//! parts (AGENTS.md §2).

use std::collections::HashSet;

use kassensturz_core::{
    Account, AccountId, AccountKind, CurrencyCode, Date, Money, Split, SplitId, Transaction,
    TransactionError, TransactionId,
};
use kassensturz_import::Statement;
use kassensturz_store::{AccountPart, Action, Book, StatedBalance, StoreError};

use crate::{ImportReport, PipelineError};

/// Why a starting balance can't be worked out.
#[derive(Debug)]
pub enum StartError {
    /// A starting balance is dated after the day the balance is for, so an
    /// earlier one would count twice.
    Later,
    /// The book refused, or an amount got too large to add up.
    Store(StoreError),
    /// The changed starting balance doesn't add up; only an amount too
    /// large can get here.
    Transaction(TransactionError),
}

impl From<StoreError> for StartError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Which balance a starting balance was worked out from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartSource {
    /// The balance the bank reported with the statement.
    Bank,
    /// The balance the user stated for the account (D-117).
    Stated,
}

/// What working out the starting balance changes, or changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartChange {
    /// The account.
    pub account: AccountId,
    /// The starting balance that changes: its amount in the account and its
    /// day. `None` if one is added.
    pub was: Option<(Money, Date)>,
    /// What it becomes: its amount in the account and its day.
    pub becomes: (Money, Date),
    /// What the account then holds, at the end of which day.
    pub holds: StatedBalance,
    /// The starting balance to change; `None` adds one.
    target: Option<(TransactionId, SplitId)>,
}

impl StartChange {
    /// Whether a starting balance is added, since the account has none that
    /// can change.
    pub fn adds(&self) -> bool {
        self.was.is_none()
    }

    /// Whether the starting balance gets another day.
    pub fn moves(&self) -> bool {
        self.was.is_some_and(|(_, day)| day != self.becomes.1)
    }
}

/// The parts of `account` that are starting balances (booked against an
/// internal account) and its other entries.
pub fn parts_by_origin(
    book: &Book,
    account: AccountId,
) -> Result<(Vec<AccountPart>, Vec<AccountPart>), StoreError> {
    let equity: HashSet<AccountId> = book
        .all_accounts()?
        .into_iter()
        .filter(|other| other.kind == AccountKind::Equity)
        .map(|other| other.id)
        .collect();
    let (mut starting, mut entries) = (Vec::new(), Vec::new());
    for part in book.account_parts(account)? {
        let is_starting = book
            .transaction(part.transaction)?
            .splits()
            .iter()
            .any(|split| split.account.is_some_and(|other| equity.contains(&other)));
        if is_starting {
            starting.push(part);
        } else {
            entries.push(part);
        }
    }
    Ok((starting, entries))
}

/// The internal account that starting balances in `currency` are booked
/// against (`docs/GLOSSARY.md`), made when there is none yet.
pub fn starting_balances_account(
    book: &mut Book,
    currency: CurrencyCode,
) -> Result<AccountId, StoreError> {
    let all = book.all_accounts()?;
    let found = all
        .iter()
        .find(|account| account.kind == AccountKind::Equity && account.currency == currency);
    if let Some(account) = found {
        return Ok(account.id);
    }
    // Names are never shared, so a second currency's account gets its code.
    let others = all
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
        number: None,
        notes: None,
        institution: None,
    })?;
    Ok(id)
}

/// How far [`plan`] may go.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// The user checks a day: a starting balance dated after it is refused.
    Checked,
    /// The first import: the account's one starting balance changes even
    /// when it is dated after the bank's day, since the bank's figure
    /// replaces what the user stated.
    FirstImport,
}

/// What it takes for `account` to hold `bank` at the end of `day` (D-111,
/// D-116): the latest starting balance dated on or before the day changes
/// by the difference, and an account without one gets one. `None` if the
/// account holds it already and nothing would change.
///
/// **The day of the starting balance (D-903):** when the account has no
/// other starting balance and an entry is dated on or before the starting
/// balance, the starting balance moves to the day before the account's
/// first entry. Its amount is worked out so that the account still holds
/// `bank` on `day`; so every day in between shows what the account really
/// held, not only the days from the starting balance on.
///
/// A starting balance dated after `day` is refused ([`StartError::Later`]),
/// since an earlier one would then count twice. Nothing is changed here.
pub fn plan_start(
    book: &Book,
    account: AccountId,
    bank: Money,
    day: Date,
) -> Result<Option<StartChange>, StartError> {
    plan(book, account, bank, day, Reach::Checked, None)
}

/// [`plan_start`], with how far it may go, and with `not_after`: a day the
/// starting balance must not be dated after when it moves, e.g. the day of
/// a statement's opening balance, which already counts it.
fn plan(
    book: &Book,
    account: AccountId,
    bank: Money,
    day: Date,
    reach: Reach,
    not_after: Option<Date>,
) -> Result<Option<StartChange>, StartError> {
    let overflow = |_| StoreError::AmountOverflow;
    let (starting, entries) = parts_by_origin(book, account)?;
    let later = starting.iter().any(|part| part.bank_date() > day);
    let target = if later {
        match (reach, starting.as_slice()) {
            (Reach::FirstImport, [only]) => Some(only),
            _ => return Err(StartError::Later),
        }
    } else {
        starting
            .iter()
            .max_by_key(|part| (part.bank_date(), part.transaction))
    };
    // What the account's parts counted on the day add up to without the
    // starting balance that changes; the rest is what that one has to be.
    let others = Money::checked_sum(
        bank.currency(),
        starting
            .iter()
            .chain(&entries)
            .filter(|part| part.bank_date() <= day)
            .filter(|part| target.is_none_or(|target| target.split != part.split))
            .map(|part| part.amount),
    )
    .map_err(overflow)?;
    let amount = bank.checked_sub(others).map_err(overflow)?;

    // Only an account's single starting balance moves, and only one that is
    // a plain transaction between the account and the internal one: moving
    // one of several, or one shared with another account, would change days
    // nobody asked about.
    let movable = match target {
        None => starting.is_empty(),
        Some(target) => {
            starting.len() == 1 && book.transaction(target.transaction)?.splits().len() == 2
        }
    };
    let base = target.map_or(day, |target| target.bank_date().min(day));
    let first = entries.iter().map(AccountPart::bank_date).min();
    let date = match first {
        Some(first) if movable && first <= base => first.yesterday().unwrap_or(first),
        _ => base,
    };
    let date = match not_after {
        Some(not_after) if movable => date.min(not_after),
        _ => date,
    };

    let was = target.map(|target| (target.amount, target.bank_date()));
    if was == Some((amount, date)) || (was.is_none() && amount.is_zero()) {
        return Ok(None);
    }
    Ok(Some(StartChange {
        account,
        was,
        becomes: (amount, date),
        holds: StatedBalance {
            amount: bank,
            on: day,
        },
        target: target.map(|target| (target.transaction, target.split)),
    }))
}

/// Does what `change` says, as one user action
/// ([`Action::CorrectStartingBalance`]; inside another action, as part of
/// it): the starting balance gets its new amount and day, or is added, every
/// other entry stays as it is, and the account keeps what it then holds as
/// its stated balance (D-117). Undo takes all of it back together (D-066).
///
/// `change` must come from this book as it is now ([`plan_start`]).
pub fn apply_start(book: &mut Book, change: &StartChange) -> Result<(), StartError> {
    let (amount, date) = change.becomes;
    book.as_one_action(Action::CorrectStartingBalance, |book| {
        match change.target {
            Some((transaction, split)) => {
                let stored = book.transaction(transaction)?;
                let was = change
                    .was
                    .map_or(Money::zero(amount.currency()), |was| was.0);
                let difference = amount
                    .checked_sub(was)
                    .map_err(|_| StoreError::AmountOverflow)?;
                let equity = equity_of(book, &stored)?;
                let mut splits = stored.splits().to_vec();
                for part in &mut splits {
                    if part.id == split {
                        part.amount = part
                            .amount
                            .checked_add(difference)
                            .map_err(|_| StoreError::AmountOverflow)?;
                    } else if part.account == Some(equity) {
                        part.amount = part
                            .amount
                            .checked_sub(difference)
                            .map_err(|_| StoreError::AmountOverflow)?;
                    }
                }
                let changed = Transaction::new(
                    stored.id(),
                    date,
                    stored.payee().map(ToOwned::to_owned),
                    splits,
                )
                .map_err(StartError::Transaction)?
                .with_details_of(&stored);
                book.update_transaction(&changed)?;
            }
            None => {
                let equity = starting_balances_account(book, amount.currency())?;
                let balancing = amount
                    .checked_neg()
                    .map_err(|_| StoreError::AmountOverflow)?;
                let transaction = Transaction::new(
                    TransactionId::generate(),
                    date,
                    None,
                    vec![
                        Split::new(change.account, amount),
                        Split::new(equity, balancing),
                    ],
                )
                .map_err(StartError::Transaction)?;
                book.add_transaction(&transaction)?;
            }
        }
        book.set_stated_balance(change.account, Some(&change.holds))?;
        Ok(())
    })
}

/// The internal account a starting-balance `transaction` is booked against.
fn equity_of(book: &Book, transaction: &Transaction) -> Result<AccountId, StoreError> {
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
        .ok_or(StoreError::UnknownTransaction(transaction.id()))
}

/// After the first import into `account`, works out its starting balance
/// from the bank's balance (D-903, B-411), inside the import's user action
/// or dry run, and says what it changed. `report` is that import's; its
/// balance checks get KassenSturz's figures as they are afterwards.
///
/// **When:** the import brought the account's first entries (before it, the
/// account held nothing but at most one starting balance), and a balance is
/// known:
///
/// - the bank's closing balance of the import, the latest one. For a
///   statement fetched from the bank (`fetched`) it always counts: it is
///   what the bank says now. For a file it counts unless the user stated a
///   balance for a later day, which is then the newer knowledge.
/// - otherwise the balance the user stated for the account (D-117).
///
/// The starting balance then is that balance minus every entry counted up
/// to its day, dated the day before the first entry ([`plan_start`]), or
/// the day of the earliest opening balance of `statements` if that is
/// earlier: the bank's opening balance is the same figure, and the import's
/// check holds it against that day. With the bank's balance, the account
/// keeps it as its stated balance.
///
/// **When not:** a later import never changes the starting balance by
/// itself: a difference then mostly means an entry is missing or there
/// twice, which the user looks into ("Check against my bank", D-111). An
/// account with several starting balances, or with no balance known, is
/// left as it is.
pub fn derive_start_after_import(
    book: &mut Book,
    account: AccountId,
    statements: &[Statement],
    report: &mut ImportReport,
    fetched: bool,
) -> Result<Option<(StartChange, StartSource)>, PipelineError> {
    let added: HashSet<TransactionId> = report.imported().into_iter().collect();
    if added.is_empty() {
        return Ok(None);
    }
    let (starting, entries) = parts_by_origin(book, account)?;
    let first_import = entries.iter().all(|part| added.contains(&part.transaction));
    if !first_import || starting.len() > 1 {
        return Ok(None);
    }
    let stated = book.stated_balance(account)?;
    let closing = report
        .balances
        .iter()
        .max_by_key(|check| check.date)
        .map(|check| StatedBalance {
            amount: check.bank,
            on: check.date,
        });
    let (known, source) = match (closing, stated) {
        (Some(closing), Some(stated)) if !fetched && stated.on > closing.on => {
            (stated, StartSource::Stated)
        }
        (Some(closing), _) => (closing, StartSource::Bank),
        (None, Some(stated)) => (stated, StartSource::Stated),
        (None, None) => return Ok(None),
    };
    let store = |error| match error {
        StartError::Store(error) => error,
        // One starting balance at most, so none is "later"; and a
        // transaction changed by a difference only fails when too large.
        StartError::Later | StartError::Transaction(_) => StoreError::AmountOverflow,
    };
    let opening = statements
        .iter()
        .filter_map(|statement| statement.opening_balance)
        .map(|opening| opening.date)
        .min();
    let Some(change) = plan(
        book,
        account,
        known.amount,
        known.on,
        Reach::FirstImport,
        opening,
    )
    .map_err(store)?
    else {
        return Ok(None);
    };
    apply_start(book, &change).map_err(store)?;
    for check in &mut report.balances {
        check.book = book.balance_on(account, check.date)?;
    }
    Ok(Some((change, source)))
}
