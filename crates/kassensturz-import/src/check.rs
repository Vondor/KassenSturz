// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks what an importer returns, so the import pipeline can rely on it
//! whichever importer read the file, a plugin's included later (D-091).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{
    AccountKind, Currency, CurrencyCode, ImportedDetails, Money, PayeeRecognition,
};

use crate::book::{BookAccount, BookTransaction, ImportedBook, Replacement};
use crate::error::ImportError;
use crate::importer::Imported;
use crate::limits::Limits;
use crate::statement::Statement;

/// Checks that `imported` keeps `limits`, and that its amounts are where
/// they belong: every amount of a statement in the statement's currency;
/// for a book, every part's amount in its transaction's currency and in
/// its account's, every key unique and every reference to one found.
pub(crate) fn check(imported: &Imported, limits: &Limits) -> Result<(), ImportError> {
    match imported {
        Imported::Book(book) => check_book(book, limits),
        Imported::Statements(statements) => {
            let entries = statements
                .iter()
                .map(|statement| statement.entries.len())
                .fold(0_usize, usize::saturating_add);
            limits.check_entries(entries)?;
            statements
                .iter()
                .try_for_each(|statement| check_statement(statement, limits))
        }
    }
}

fn check_statement(statement: &Statement, limits: &Limits) -> Result<(), ImportError> {
    let expected = statement.currency.code();
    let in_currency = |amount: Money| {
        if amount.currency() == expected {
            Ok(())
        } else {
            Err(ImportError::CurrencyMismatch {
                expected,
                found: amount.currency(),
            })
        }
    };
    let account = &statement.account;
    for text in [&account.iban, &account.number].into_iter().flatten() {
        limits.check_text(text, None)?;
    }
    for balance in [statement.opening_balance, statement.closing_balance]
        .into_iter()
        .flatten()
    {
        in_currency(balance.amount)?;
    }
    for entry in &statement.entries {
        in_currency(entry.amount)?;
        for text in entry.texts() {
            limits.check_text(text, None)?;
        }
    }
    Ok(())
}

/// The checks of a whole book. A mistake an importer could only make by
/// not keeping to [`ImportedBook`]'s rules, such as a part booked to a key
/// no account has, is `import.malformed`: the file or the importer is
/// broken, and the pipeline mustn't guess.
fn check_book(book: &ImportedBook, limits: &Limits) -> Result<(), ImportError> {
    let malformed = || ImportError::Malformed { line: None };
    limits.check_entries(book.transactions.len())?;
    // Each currency is read in one number of decimal places throughout.
    let mut currencies: HashMap<CurrencyCode, u8> = HashMap::new();
    let mut uses =
        |currency: Currency| match currencies.insert(currency.code(), currency.minor_digits()) {
            Some(earlier) if earlier != currency.minor_digits() => {
                Err(ImportError::DecimalPlacesDiffer {
                    currency: currency.code(),
                    book: earlier,
                    file: currency.minor_digits(),
                })
            }
            _ => Ok(()),
        };
    let mut accounts: HashMap<&str, &BookAccount> = HashMap::new();
    for account in &book.accounts {
        limits.check_text(&account.key, None)?;
        limits.check_text(&account.name, None)?;
        uses(account.currency)?;
        let fits_kind = match account.account_type {
            Some(account_type) => account_type.kind() == account.kind,
            None => true,
        };
        if !fits_kind || accounts.insert(&account.key, account).is_some() {
            return Err(malformed());
        }
    }
    let mut institutions = HashSet::new();
    for institution in &book.institutions {
        for text in [Some(&institution.key), Some(&institution.name)]
            .into_iter()
            .chain([
                institution.bank_code.as_ref(),
                institution.contact.as_ref(),
                institution.address.as_ref(),
                institution.phone.as_ref(),
            ])
            .flatten()
        {
            limits.check_text(text, None)?;
        }
        if !institutions.insert(institution.key.as_str()) {
            return Err(malformed());
        }
    }
    let mut payees = HashSet::new();
    for payee in &book.payees {
        let key_words = match &payee.recognition {
            PayeeRecognition::ByKeyWords(words) => words.as_slice(),
            PayeeRecognition::Never | PayeeRecognition::ByName => &[],
        };
        for text in [Some(&payee.key), Some(&payee.name)]
            .into_iter()
            .chain([
                payee.address.as_ref(),
                payee.email.as_ref(),
                payee.phone.as_ref(),
                payee.reference.as_ref(),
            ])
            .flatten()
            .chain(key_words)
        {
            limits.check_text(text, None)?;
        }
        if !payees.insert(payee.key.as_str()) {
            return Err(malformed());
        }
    }
    for rule in &book.rules {
        // A Rule has a name, a payee to set and a condition, and books to
        // an account the file has.
        if rule.name.trim().is_empty()
            || rule.payee.trim().is_empty()
            || rule.conditions.is_empty()
            || rule.conditions.iter().any(|c| c.value.trim().is_empty())
            || rule
                .book_to
                .as_ref()
                .is_some_and(|key| !accounts.contains_key(key.as_str()))
        {
            return Err(malformed());
        }
        for text in [&rule.name, &rule.payee]
            .into_iter()
            .chain(rule.conditions.iter().map(|c| &c.value))
        {
            limits.check_text(text, None)?;
        }
    }
    let mut tags = HashSet::new();
    for tag in &book.tags {
        limits.check_text(&tag.key, None)?;
        limits.check_text(&tag.name, None)?;
        if !tags.insert(tag.key.as_str()) {
            return Err(malformed());
        }
    }
    for account in &book.accounts {
        if let Some(parent) = &account.parent {
            let parent = accounts.get(parent.as_str()).ok_or_else(malformed)?;
            // A category sits in a category, an account in an account, each
            // of its own kind (D-082, D-132).
            if account.kind == AccountKind::Equity || parent.kind != account.kind {
                return Err(malformed());
            }
        }
        if let Some(institution) = &account.institution
            && (!account.kind.is_account() || !institutions.contains(institution.as_str()))
        {
            return Err(malformed());
        }
        if account.number.is_some() && !account.kind.is_account() {
            return Err(malformed());
        }
        for text in [&account.number, &account.notes].into_iter().flatten() {
            limits.check_text(text, None)?;
        }
    }
    for transaction in &book.transactions {
        uses(transaction.currency)?;
        check_transaction(transaction, &accounts, &tags, limits)?;
    }
    for rate in &book.rates {
        if rate.from == rate.to {
            return Err(malformed());
        }
    }
    // The unlisted currencies are those of the book's currencies that
    // neither of KassenSturz's tables lists, each once, and a suggestion is
    // a current currency (D-106).
    let unlisted: HashSet<CurrencyCode> = book
        .unlisted
        .iter()
        .map(|unlisted| unlisted.currency.code())
        .collect();
    let listed =
        |code: CurrencyCode| code.iso_minor_digits().is_some() || code.withdrawn().is_some();
    let needed: HashSet<CurrencyCode> = currencies
        .keys()
        .copied()
        .filter(|&code| !listed(code))
        .collect();
    if unlisted.len() != book.unlisted.len() || unlisted != needed {
        return Err(malformed());
    }
    for unlisted in &book.unlisted {
        let currency = unlisted.currency;
        let current = |suggestion: Replacement| suggestion.successor.iso_minor_digits().is_some();
        if currencies.get(&currency.code()) != Some(&currency.minor_digits())
            || !unlisted.suggestion.is_none_or(current)
        {
            return Err(malformed());
        }
    }
    Ok(())
}

/// One transaction of a book: its texts within the limits, every amount in
/// the transaction's currency, and every part's account known, with its
/// amount in that account's currency where it differs.
fn check_transaction(
    transaction: &BookTransaction,
    accounts: &HashMap<&str, &BookAccount>,
    tags: &HashSet<&str>,
    limits: &Limits,
) -> Result<(), ImportError> {
    let malformed = || ImportError::Malformed { line: None };
    let currency = transaction.currency.code();
    for text in [&transaction.payee, &transaction.note]
        .into_iter()
        .flatten()
    {
        limits.check_text(text, None)?;
    }
    for part in &transaction.parts {
        for text in [&part.note, &part.number].into_iter().flatten() {
            limits.check_text(text, None)?;
        }
        // Each tag once, and one the file lists (D-132).
        for (index, tag) in part.tags.iter().enumerate() {
            if !tags.contains(tag.as_str()) || part.tags[..index].contains(tag) {
                return Err(malformed());
            }
        }
        if part.amount.currency() != currency {
            return Err(ImportError::CurrencyMismatch {
                expected: currency,
                found: part.amount.currency(),
            });
        }
        let Some(key) = &part.account else {
            // A part without a category counts in the transaction's
            // currency and has no bank behind it (D-083).
            if part.account_amount.is_some() || part.details.is_some() {
                return Err(malformed());
            }
            continue;
        };
        let account = accounts.get(key.as_str()).ok_or_else(malformed)?;
        let account_currency = account.currency.code();
        match part.account_amount {
            None if account_currency == currency => {}
            Some(amount)
                if account_currency != currency && amount.currency() == account_currency =>
            {
                // Both amounts point the same way: rates are above zero.
                let opposite = (part.amount.is_negative() && amount.minor_units() > 0)
                    || (part.amount.minor_units() > 0 && amount.is_negative());
                if opposite {
                    return Err(malformed());
                }
            }
            _ => {
                return Err(ImportError::CurrencyMismatch {
                    expected: account_currency,
                    found: part
                        .account_amount
                        .map_or(currency, |amount| amount.currency()),
                });
            }
        }
        if let Some(details) = &part.details {
            for text in detail_texts(details) {
                limits.check_text(text, None)?;
            }
        }
    }
    Ok(())
}

/// Every text of what a file delivered for a part.
fn detail_texts(details: &ImportedDetails) -> impl Iterator<Item = &str> {
    [
        &details.bank_id,
        &details.other_party_name,
        &details.other_party_iban,
        &details.remittance_information,
    ]
    .into_iter()
    .filter_map(|text| text.as_deref())
}

#[cfg(test)]
mod tests;
