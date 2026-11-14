// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding what a command names: accounts and categories by name,
//! transactions by the end of their ID.

use std::collections::BTreeSet;

use kassensturz_core::{Account, TransactionId};
use kassensturz_store::{Book, name_key, stored_name};

use crate::error::CliError;
use crate::text::plain;

/// The ID as the tool shows it: the last 12 characters. A UUIDv7 begins with
/// the time, so its start is the same for IDs made within a minute, but its
/// end is random (D-067).
pub(crate) fn short_id(id: impl std::fmt::Display) -> String {
    let full = id.to_string();
    full[full.len().saturating_sub(12)..].to_owned()
}

/// What users type to find something is compared as the book compares
/// names (D-081, D-782): ignoring capitals, spaces around it, characters
/// that show as nothing, and how an accent or a ligature was typed, so
/// "Gir\u{F3}" typed as "Giro" and a combining accent, as macOS often
/// sends it, finds "Gir\u{F3}".
pub(crate) fn normalized(text: &str) -> String {
    name_key(text)
}

/// `text` as typed, only composed and ignoring capitals: to choose between
/// names that look the same, which a book made before D-782 may hold.
fn as_typed(text: &str) -> String {
    stored_name(text.trim()).to_lowercase()
}

/// Of `found`, the ones named `text` as typed if there are several.
pub(crate) fn as_typed_if_several<'a, T>(
    found: Vec<&'a T>,
    text: &str,
    name: impl Fn(&T) -> &str,
) -> Vec<&'a T> {
    if found.len() < 2 {
        return found;
    }
    let typed = as_typed(text);
    let exact: Vec<&T> = found
        .iter()
        .copied()
        .filter(|item| as_typed(name(item)) == typed)
        .collect();
    if exact.is_empty() { found } else { exact }
}

/// The user's account called `name`, closed ones included.
pub(crate) fn find_account(book: &Book, name: &str) -> Result<Account, CliError> {
    let wanted = normalized(name);
    let accounts = book.accounts(true)?;
    let found: Vec<&Account> = accounts
        .iter()
        .filter(|account| normalized(&account.name) == wanted)
        .collect();
    match as_typed_if_several(found, name, |account| &account.name).as_slice() {
        [account] => Ok((*account).clone()),
        [] => Err(CliError::message(format!(
            "there is no account called `{}`. `account list --all` shows the accounts",
            plain(name)
        ))),
        several => Err(CliError::message(format!(
            "`{}` could be several accounts: {}. Type the name exactly as `account list --all` shows it",
            plain(name),
            several
                .iter()
                .map(|account| plain(&account.name))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
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
    matches = as_typed_if_several(matches, text, |(_, path)| path);
    if matches.is_empty() {
        matches = candidates
            .iter()
            .filter(|(account, _)| normalized(&account.name) == wanted)
            .collect();
        matches = as_typed_if_several(matches, text, |(account, _)| &account.name);
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
