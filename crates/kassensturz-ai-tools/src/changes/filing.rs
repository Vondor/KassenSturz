// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A bill from the receipt inbox, filed into the archive and linked to a
//! transaction by the user's own AI tool (US7, #333, D-751, D-752,
//! `docs/RECEIPT_INBOX.md` §3 and §4).
//!
//! Behind its own cell, *Transactions › File bills from the inbox*, off by
//! default, and offered over MCP only while the user has chosen both an
//! inbox and an archive. The AI tool reads the bill itself; KassenSturz
//! reads no bill. It checks that the file is a PDF, PNG or JPEG file right
//! in the inbox (no subfolder, no link), records its size and time, and
//! checks the total the AI read against what the bank booked, in exact
//! minor units: a bill that doesn't match starts left out, so only the
//! user can file it (D-751).
//!
//! **There is no destination.** Where the bill goes is built by the app
//! from the book's own names when the user confirms
//! (`kassensturz_core::receipt_inbox`), and the app moves the file
//! (`kassensturz_platform::bill_files`); a set of this kind can't be
//! applied before the app has put each bill in place
//! ([`FileBill::archived`]).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use kassensturz_core::{AccountId, Attachment, CurrencyCode, Date, Money, TransactionId};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::bills::{BILL_TYPES, bill_path};
use super::transactions::{free_text, parse_amount};
use super::{ChangeError, ChangeSet, Known, Object, ProposedChange};
use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};

/// The list of `propose_changes` that holds the bills to file.
pub(super) const LIST: &str = "file_bills";

/// The most bills one set may file, so the user can still review them.
pub(super) const MAX_FILED: usize = 50;

/// The largest bill that is filed, in bytes.
pub const MAX_BILL_BYTES: u64 = 50 * 1024 * 1024;

/// The most bytes one set may file, all bills together.
pub const MAX_FILED_BYTES: u64 = 500 * 1024 * 1024;

/// The most files [`INBOX`] lists.
const MAX_LISTED: usize = 200;

/// The function that lists the receipt inbox (D-752). Only the MCP server
/// offers it, and only while both folders are chosen and Transactions ›
/// File bills from the inbox is on, so no path goes to a model the user
/// didn't allow.
pub const INBOX: Function = Function {
    name: "inbox",
    description: "Lists the bills waiting in the user's receipt inbox: the folder's path and \
        each file's name, size and date. Read a bill yourself, find the transaction it was \
        paid with (search_transactions, with reference for an invoice or order number), and \
        file it with file_bills in propose_changes, with the total the bill shows. When the \
        user confirms, KassenSturz moves the file into the archive by the transaction's \
        account and category; you don't choose the place. KassenSturz never reads the bills.",
    parameters: r#"{"type":"object","properties":{},"additionalProperties":false}"#,
};

/// What [`INBOX`] answers for `inbox`: its path as the user chose it and
/// its files that can be filed: plain files right in it with one of the
/// [`BILL_TYPES`], at most a few hundred, sorted by name. A subfolder is
/// not entered and a link is not followed or listed.
pub fn inbox_answer(inbox: &Path, arguments: &Value) -> Result<Output, ToolError> {
    match arguments {
        Value::Null => {}
        Value::Object(values) if values.is_empty() => {}
        _ => {
            return Err(ToolError::BadArgument {
                argument: "arguments",
            });
        }
    }
    let folder = inbox.to_str().ok_or(ToolError::NoInbox)?.to_owned();
    let entries = std::fs::read_dir(inbox).map_err(|_| ToolError::NoInbox)?;
    let mut files: Vec<(String, u64, Option<String>)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            // `file_type` and `metadata` of a directory entry don't follow
            // a link.
            let meta = entry.metadata().ok()?;
            if !meta.is_file() || !has_bill_type(&name) {
                return None;
            }
            let modified = meta.modified().ok().and_then(|time| {
                let since = time.duration_since(UNIX_EPOCH).ok()?;
                let seconds = i64::try_from(since.as_secs()).ok()?;
                let stamp = kassensturz_core::Timestamp::from_second(seconds).ok()?;
                Some(stamp.to_string())
            });
            Some((name, meta.len(), modified))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let total = files.len();
    files.truncate(MAX_LISTED);
    let listed: Vec<Value> = files
        .into_iter()
        .map(|(name, size, modified)| {
            let mut file = serde_json::Map::new();
            file.insert("name".into(), Value::String(name));
            file.insert("size_bytes".into(), Value::from(size));
            if let Some(modified) = modified {
                file.insert("modified".into(), Value::String(modified));
            }
            Value::Object(file)
        })
        .collect();
    let mut figures = Figures::new();
    figures.add_count(u64::try_from(total).unwrap_or(u64::MAX));
    let mut answer = serde_json::json!({
        "folder": folder,
        "file_types": BILL_TYPES,
        "files": listed,
    });
    if total > MAX_LISTED {
        answer["files_left_out"] = Value::from(total - MAX_LISTED);
    }
    Ok(Output::new(answer, figures))
}

/// Whether a name ends in one of the [`BILL_TYPES`], without case.
fn has_bill_type(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ending| ending.to_str())
        .is_some_and(|ending| {
            BILL_TYPES
                .iter()
                .any(|kind| ending.eq_ignore_ascii_case(kind))
        })
}

/// The keys an item of [`LIST`] takes. No destination: the place is the
/// app's (§3).
const KEYS: &[&str] = &[
    "transaction_id",
    "file",
    "title",
    "bill_total",
    "bill_currency",
];

/// How the total the AI read on the bill compares with the bank's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TotalCheck {
    /// The AI gave no total: the user checks the bill themselves.
    NotGiven,
    /// The same amount in the same currency, to the cent.
    Matches,
    /// Another amount, or another currency: the bill starts left out.
    Differs,
}

/// A bill from the inbox to file and link to a transaction, as the user
/// reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileBill {
    /// The transaction.
    pub transaction: TransactionId,
    /// The day its account shows it with: the archive's file name starts
    /// with it.
    pub date: Date,
    /// Who was paid or paid, if anyone.
    pub payee: Option<String>,
    /// The user's account the money moved in or out of.
    pub account: AccountId,
    /// That account's name: the archive's first folder.
    pub account_name: String,
    /// The category of the transaction's largest part, if it has one: the
    /// archive's second folder (§5).
    pub category: Option<String>,
    /// The bill in the inbox, with links resolved.
    pub file: PathBuf,
    /// Its file name, as the check card shows it.
    pub file_name: String,
    /// What the link is called and the archived file's name: the model's
    /// title, or `None` for the file's own name.
    pub title: Option<String>,
    /// Its size in bytes when the AI proposed it.
    pub size: u64,
    /// Its time of last change then, in nanoseconds since 1970.
    pub modified: Option<u64>,
    /// What the bank booked on the account, as the account shows it.
    pub booked: Money,
    /// The total the AI read on the bill, exact, if it gave one.
    pub bill_total: Option<Money>,
    /// How the two compare.
    pub check: TotalCheck,
    /// Where the app put the bill in the archive, once the user confirmed;
    /// `None` until then, and a set with a bill not yet put in place is not
    /// applied.
    pub archived: Option<String>,
}

impl FileBill {
    /// The name the archived file takes, without its ending: the title,
    /// or the file's own stem.
    pub fn name(&self) -> String {
        self.title.clone().unwrap_or_else(|| {
            Path::new(&self.file_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_owned()
        })
    }

    /// The file's ending, as it is.
    pub fn ending(&self) -> &str {
        Path::new(&self.file_name)
            .extension()
            .and_then(|ending| ending.to_str())
            .unwrap_or_default()
    }
}

/// The two folders of the receipt inbox, as the user chose them on this
/// computer.
#[derive(Clone, Copy, Debug)]
pub(super) struct InboxFolders<'a> {
    pub(super) inbox: Option<&'a Path>,
    pub(super) archive: Option<&'a Path>,
}

impl Known {
    /// Checks one item of [`LIST`] against `book` and the folders.
    /// `filed` holds the bills this set files already, and `bytes` their
    /// sizes so far.
    pub(super) fn file_bill(
        &self,
        book: &Book,
        value: &Value,
        folders: InboxFolders<'_>,
        filed: &mut HashSet<PathBuf>,
        bytes: &mut u64,
    ) -> Result<FileBill, ToolError> {
        let item = Object::read(value, KEYS, LIST)?;
        let id = item
            .uuid("transaction_id")?
            .map(TransactionId::from_bytes)
            .ok_or(ToolError::BadArgument {
                argument: "transaction_id",
            })?;
        let inbox = folders.inbox.ok_or(ToolError::NoInbox)?;
        folders.archive.ok_or(ToolError::NoArchive)?;
        let stored = match book.transaction(id) {
            Err(StoreError::UnknownTransaction(_)) => return Err(ToolError::UnknownTransaction),
            other => other?,
        };
        let given = item.required_string("file")?;
        let file = bill_path(inbox, given).map_err(|error| match error {
            ToolError::NoBillFolder => ToolError::NoInbox,
            other => other,
        })?;
        // Right in the inbox: no subfolder, and not through a link.
        let real_inbox = inbox.canonicalize().map_err(|_| ToolError::NoInbox)?;
        if file.parent() != Some(real_inbox.as_path()) {
            return Err(ToolError::BillOutsideFolder);
        }
        let named = if Path::new(given).is_absolute() {
            PathBuf::from(given)
        } else {
            inbox.join(given)
        };
        if std::fs::symlink_metadata(&named).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(ToolError::BillOutsideFolder);
        }
        let meta = std::fs::symlink_metadata(&file).map_err(|_| ToolError::BillNotFound)?;
        if !meta.is_file() {
            return Err(ToolError::BillNotFound);
        }
        let size = meta.len();
        *bytes = bytes.saturating_add(size);
        if size > MAX_BILL_BYTES || *bytes > MAX_FILED_BYTES {
            return Err(ToolError::BillTooBig);
        }
        let link = file.to_str().ok_or(ToolError::BillNotFound)?;
        if !book.transactions_linking(link)?.is_empty() {
            return Err(ToolError::BillAlreadyFiled);
        }
        if !filed.insert(file.clone()) {
            return Err(ToolError::Duplicate);
        }
        let title = item
            .string("title")?
            .map(|text| free_text(text, "title"))
            .transpose()?
            .flatten();
        // The user's own account: the bill is filed under it.
        let own = stored
            .splits()
            .iter()
            .find(|split| {
                split
                    .account
                    .and_then(|account| self.accounts.get(&account))
                    .is_some_and(|account| account.kind.is_account())
            })
            .ok_or(ToolError::NotEditable {
                reason: "no-account",
            })?;
        let account = own.account.ok_or(ToolError::UnknownAccount)?;
        let account_name = self
            .accounts
            .get(&account)
            .map(|account| account.name.clone())
            .unwrap_or_default();
        // The category of the largest other part, the first on a tie.
        let mut largest: Option<(i64, Option<String>)> = None;
        for split in stored.splits() {
            if std::ptr::eq(split, own) {
                continue;
            }
            let size = split.amount.minor_units().unsigned_abs();
            let size = i64::try_from(size).unwrap_or(i64::MAX);
            if largest.as_ref().is_none_or(|(most, _)| size > *most) {
                let category = split
                    .account
                    .and_then(|account| self.accounts.get(&account))
                    .filter(|account| account.kind.is_category())
                    .map(|account| account.name.clone());
                largest = Some((size, category));
            }
        }
        let category = largest.and_then(|(_, category)| category);
        let booked_on_account = own
            .foreign
            .as_ref()
            .map_or(own.amount, |foreign| foreign.amount);
        let (bill_total, check) = bill_total(book, &item, own.amount, booked_on_account)?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|since| u64::try_from(since.as_nanos()).ok());
        Ok(FileBill {
            transaction: id,
            date: stored.date_of(own),
            payee: stored.payee_of(own).map(str::to_owned),
            account,
            account_name,
            category,
            file_name: file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            file,
            title,
            size,
            modified,
            booked: booked_on_account,
            bill_total,
            check,
            archived: None,
        })
    }
}

/// The total the AI read on the bill, parsed exactly in its currency
/// (`bill_currency`, else the transaction's), and how it compares with
/// what the bank booked: the transaction's amount (`amount`) or the
/// account's (`on_account`, for an account in another currency), without
/// their sign (D-751, §4). More decimal places than the currency has is a
/// refusal, never rounded.
fn bill_total(
    book: &Book,
    item: &Object<'_>,
    amount: Money,
    on_account: Money,
) -> Result<(Option<Money>, TotalCheck), ToolError> {
    let bad = |argument| ToolError::BadArgument { argument };
    let currency = match item.string("bill_currency")? {
        Some(code) => CurrencyCode::new(code).map_err(|_| bad("bill_currency"))?,
        None => amount.currency(),
    };
    let Some(text) = item.string("bill_total")? else {
        return Ok((None, TotalCheck::NotGiven));
    };
    let digits = match book.currency_minor_digits(currency) {
        Ok(digits) => digits,
        Err(StoreError::UnknownCurrency(_)) => currency
            .iso_minor_digits()
            .ok_or_else(|| bad("bill_currency"))?,
        Err(other) => return Err(other.into()),
    };
    let total = parse_amount(text, currency, digits).map_err(|_| bad("bill_total"))?;
    let unsigned = |money: Money| money.minor_units().unsigned_abs();
    let matches = [amount, on_account]
        .into_iter()
        .any(|booked| booked.currency() == total.currency() && unsigned(booked) == unsigned(total));
    let check = if matches {
        TotalCheck::Matches
    } else {
        TotalCheck::Differs
    };
    Ok((Some(total), check))
}

/// Links the bill the app has put into the archive, inside the set's user
/// action, after the bills the transaction has. A bill not put in place,
/// a transaction that is gone, or one that has the link already, leaves
/// nothing of the set.
pub(super) fn apply(book: &mut Book, change: &FileBill) -> Result<(), ChangeError> {
    let link = change.archived.clone().ok_or(ChangeError::OutOfDate)?;
    let stored = match book.transaction(change.transaction) {
        Ok(stored) => stored,
        Err(StoreError::UnknownTransaction(_)) => return Err(ChangeError::OutOfDate),
        Err(error) => return Err(error.into()),
    };
    if stored
        .attachments()
        .iter()
        .any(|attachment| attachment.link == link)
    {
        return Err(ChangeError::OutOfDate);
    }
    let mut attachments = stored.attachments().to_vec();
    attachments.push(Attachment {
        link,
        title: change.title.clone(),
    });
    book.update_transaction(&stored.with_attachments(attachments))?;
    Ok(())
}

impl ChangeSet {
    /// The bills of the set to file from the inbox, in order, for the app
    /// to put in place when the user confirms.
    pub fn bills_to_file(&self) -> impl Iterator<Item = &FileBill> {
        self.changes.iter().filter_map(|change| match change {
            ProposedChange::FileBill(bill) => Some(bill),
            _ => None,
        })
    }

    /// Records where the app put each bill to file (`placed` answers the
    /// archive path, or `None` for one that stayed in the inbox) and drops
    /// the bills that stayed, so the rest of the set is applied without
    /// them: the user confirmed the set, so a failed file's row is dropped,
    /// never guessed (`docs/RECEIPT_INBOX.md` §8).
    pub fn put_bills_in_place(&mut self, mut placed: impl FnMut(&FileBill) -> Option<String>) {
        let mut keep = Vec::with_capacity(self.changes.len());
        for change in &mut self.changes {
            keep.push(match change {
                ProposedChange::FileBill(bill) => {
                    bill.archived = placed(bill);
                    bill.archived.is_some()
                }
                _ => true,
            });
        }
        let mut kept = keep.iter();
        self.changes
            .retain(|_| kept.next().copied().unwrap_or(false));
        let mut kept = keep.iter();
        self.slots.retain(|_| kept.next().copied().unwrap_or(false));
    }
}
