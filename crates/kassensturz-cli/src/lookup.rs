// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding what a command names: accounts and categories by name,
//! transactions by the end of their ID.

use std::collections::BTreeSet;

use kassensturz_core::{Account, TransactionId};
use kassensturz_store::Book;

use crate::error::CliError;
use crate::text::plain;

/// The ID as the tool shows it: the last 12 characters. A UUIDv7 begins with
/// the time, so its start is the same for IDs made within a minute, but its
/// end is random (D-067).
pub(crate) fn short_id(id: impl std::fmt::Display) -> String {
    let full = id.to_string();
    full[full.len().saturating_sub(12)..].to_owned()
}

/// What users type to find something is compared ignoring capitals and
/// spaces around it, like KassenSturz compares account names (D-081).
fn normalized(text: &str) -> String {
    text.trim().to_lowercase()
}

/// The user's account called `name`, closed ones included.
pub(crate) fn find_account(book: &Book, name: &str) -> Result<Account, CliError> {
    let wanted = normalized(name);
    let mut found = book
        .accounts(true)?
        .into_iter()
        .filter(|account| normalized(&account.name) == wanted);
    match (found.next(), found.next()) {
        (Some(account), None) => Ok(account),
        (None, _) => Err(CliError::message(format!(
            "there is no account called `{}`. `account list --all` shows the accounts",
            plain(name)
        ))),
        (Some(_), Some(_)) => unreachable!("the book keeps account names apart (D-081)"),
    }
}

/// The category called `name`: its whole path from the top, such as
/// `Food/Groceries`, or just its own name if no other category has it.
pub(crate) fn find_category(book: &Book, name: &str) -> Result<Account, CliError> {
    let candidates = category_candidates(book)?;
    resolve(candidates, name, "category", "`category list` shows them")
}

/// An account or a category called `name`, in the way both are found.
pub(crate) fn find_any(book: &Book, name: &str) -> Result<Account, CliError> {
    let mut candidates: Vec<(Account, String)> = book
        .accounts(true)?
        .into_iter()
        .map(|account| {
            let path = account.name.clone();
            (account, path)
        })
        .collect();
    candidates.extend(category_candidates(book)?);
    resolve(
        candidates,
        name,
        "account or category",
        "`account list --all` and `category list` show them",
    )
}

fn category_candidates(book: &Book) -> Result<Vec<(Account, String)>, CliError> {
    Ok(book
        .categories()?
        .into_iter()
        .map(|entry| (entry.category, entry.path.join("/")))
        .collect())
}

/// The one candidate whose path is `text`, else the one whose own name is.
fn resolve(
    candidates: Vec<(Account, String)>,
    text: &str,
    what: &str,
    hint: &str,
) -> Result<Account, CliError> {
    let wanted = normalized(text);
    let mut matches: Vec<&(Account, String)> = candidates
        .iter()
        .filter(|(_, path)| normalized(path) == wanted)
        .collect();
    if matches.is_empty() {
        matches = candidates
            .iter()
            .filter(|(account, _)| normalized(&account.name) == wanted)
            .collect();
    }
    match matches.as_slice() {
        [] => Err(CliError::message(format!(
            "there is no {what} called `{}`; {hint}",
            plain(text)
        ))),
        [(account, _)] => Ok(account.clone()),
        several => Err(CliError::message(format!(
            "`{}` could be several: {}. Type the whole path, such as Food/Other",
            plain(text),
            several
                .iter()
                .map(|(_, path)| plain(path))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// The transaction whose ID ends with `text`, at least 6 characters of it.
/// The register shows the last 12.
pub(crate) fn find_transaction(book: &Book, text: &str) -> Result<TransactionId, CliError> {
    let wanted = text.trim().to_lowercase();
    let is_id_text = wanted.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    if wanted.len() < 6 || !is_id_text {
        return Err(CliError::message(format!(
            "`{}` isn't a transaction ID. Type the last characters of the ID the register shows",
            plain(text)
        )));
    }
    let mut ids = BTreeSet::new();
    let mut places: Vec<Account> = book.accounts(true)?;
    places.extend(book.categories()?.into_iter().map(|entry| entry.category));
    for place in places {
        ids.extend(
            book.register(place.id)?
                .into_iter()
                .map(|entry| entry.transaction),
        );
    }
    let mut matches = ids
        .into_iter()
        .filter(|id| id.to_string().ends_with(&wanted));
    match (matches.next(), matches.next()) {
        (Some(id), None) => Ok(id),
        (None, _) => Err(CliError::message(format!(
            "no transaction has an ID ending in `{wanted}`"
        ))),
        (Some(_), Some(_)) => Err(CliError::message(format!(
            "several transactions have an ID ending in `{wanted}`. Type more of the ID"
        ))),
    }
}
