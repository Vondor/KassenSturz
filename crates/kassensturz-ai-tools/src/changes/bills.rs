// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A bill from the bill folder linked to a transaction by the user's own
//! AI tool (US5.13, D-739, `docs/AI_SPLITS.md` §3).
//!
//! Behind its own cell, *Transactions › Attach bills*, off by default, and
//! offered over MCP only while the user has chosen a bill folder. The AI
//! tool outside KassenSturz reads the bill itself; KassenSturz reads no
//! file and runs none. It only checks that the file the model names is a
//! PDF, PNG or JPEG file inside the folder the user chose, once `..` and
//! links are resolved, and stores the link in the transaction's
//! attachments (D-132). It never moves, changes or deletes the file.
//!
//! A transaction keeps every bill it has; the new one comes after them. A
//! bill is attached after a split in the same set, so one Undo takes back
//! both.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use kassensturz_core::{Attachment, Date, TransactionId};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::transactions::free_text;
use super::{ChangeError, Known, Object};
use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};

/// The list of `propose_changes` that holds the bills.
pub(super) const LIST: &str = "attach_bills";

/// The most bills one set may attach, so the user can still review them.
pub(super) const MAX_BILLS: usize = 100;

/// The file types a bill may have, by the end of its name, compared
/// without case.
pub const BILL_TYPES: [&str; 4] = ["pdf", "png", "jpg", "jpeg"];

/// The function that names the bill folder (D-739). Only the MCP server
/// offers it, and only while a bill folder is chosen and Transactions ›
/// Attach bills is on, so a path never goes to a model the user didn't
/// allow.
pub const BILL_FOLDER: Function = Function {
    name: "bill_folder",
    description: "Names the folder the user chose for their bills, and the file types a bill \
        may have there. Read a bill there yourself, find the transaction it was paid with by \
        its invoice or order number (search_transactions with reference), and link it with \
        attach_bills in propose_changes. KassenSturz never reads the bills.",
    parameters: r#"{"type":"object","properties":{},"additionalProperties":false}"#,
};

/// What [`BILL_FOLDER`] answers for `folder`: its path, as the user chose
/// it, and the [`BILL_TYPES`].
pub fn bill_folder_answer(folder: &Path, arguments: &Value) -> Result<Output, ToolError> {
    match arguments {
        Value::Null => {}
        Value::Object(values) if values.is_empty() => {}
        _ => {
            return Err(ToolError::BadArgument {
                argument: "arguments",
            });
        }
    }
    let folder = folder.to_str().ok_or(ToolError::NoBillFolder)?.to_owned();
    Ok(Output::new(
        serde_json::json!({"folder": folder, "file_types": BILL_TYPES}),
        Figures::new(),
    ))
}

/// The keys an item of [`LIST`] takes.
const KEYS: &[&str] = &["transaction_id", "file", "title"];

/// A bill to link to a transaction, as the user reviews it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachBill {
    /// The transaction.
    pub transaction: TransactionId,
    /// The day its account shows it with.
    pub date: Date,
    /// Who was paid or paid, if anyone.
    pub payee: Option<String>,
    /// The user's account the money moved in or out of, by its name, if
    /// it has one.
    pub account: String,
    /// The bill's file name, as the check card shows it.
    pub file_name: String,
    /// What the transaction keeps: the bill's path with links resolved.
    pub link: String,
    /// What the link is called, if the model named it; the page shows the
    /// file name otherwise.
    pub title: Option<String>,
}

impl Known {
    /// Checks one item of [`LIST`] against `book` and the bill folder
    /// `folder` (none chosen: `ai-tools.no-bill-folder`). `linked` holds
    /// the bills this set attaches already, so the same one isn't
    /// attached twice.
    pub(super) fn attach_bill(
        &self,
        book: &Book,
        value: &Value,
        folder: Option<&Path>,
        linked: &mut HashSet<(TransactionId, String)>,
    ) -> Result<AttachBill, ToolError> {
        let item = Object::read(value, KEYS, LIST)?;
        let id = item
            .uuid("transaction_id")?
            .map(TransactionId::from_bytes)
            .ok_or(ToolError::BadArgument {
                argument: "transaction_id",
            })?;
        let folder = folder.ok_or(ToolError::NoBillFolder)?;
        let stored = match book.transaction(id) {
            Err(StoreError::UnknownTransaction(_)) => return Err(ToolError::UnknownTransaction),
            other => other?,
        };
        let path = bill_path(folder, item.required_string("file")?)?;
        let link = path
            .to_str()
            .ok_or(ToolError::BadArgument { argument: "file" })?
            .to_owned();
        let title = item
            .string("title")?
            .map(|text| free_text(text, "title"))
            .transpose()?
            .flatten();
        if stored
            .attachments()
            .iter()
            .any(|attachment| attachment.link == link)
            || !linked.insert((id, link.clone()))
        {
            return Err(ToolError::Duplicate);
        }
        let own = stored.splits().iter().find(|split| {
            split
                .account
                .and_then(|account| self.accounts.get(&account))
                .is_some_and(|account| account.kind.is_account())
        });
        let account = own
            .and_then(|split| split.account)
            .and_then(|account| self.accounts.get(&account))
            .map(|account| account.name.clone())
            .unwrap_or_default();
        let (date, payee) = match own {
            Some(split) => (stored.date_of(split), stored.payee_of(split)),
            None => (stored.booking_date(), stored.payee()),
        };
        Ok(AttachBill {
            transaction: id,
            date,
            payee: payee.map(str::to_owned),
            account,
            file_name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            link,
            title,
        })
    }
}

/// The bill `file` names in `folder`, a path inside it or one relative to
/// it, with `..` and links resolved: it must stay inside the folder, have
/// one of the [`BILL_TYPES`] and be a file there. Outside the folder is
/// told before whether a file exists, so the model learns nothing of the
/// rest of the computer.
pub fn bill_path(folder: &Path, file: &str) -> Result<PathBuf, ToolError> {
    if file.is_empty() || file.contains('\0') {
        return Err(ToolError::BadArgument { argument: "file" });
    }
    let given = Path::new(file);
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        folder.join(given)
    };
    // The folder as chosen and as it is on disk: a path the model got from
    // `bill_folder` may use either.
    let real_folder = folder.canonicalize().map_err(|_| ToolError::NoBillFolder)?;
    let chosen = lexical(folder).ok_or(ToolError::NoBillFolder)?;
    let named = lexical(&joined).ok_or(ToolError::BillOutsideFolder)?;
    let inside = |path: &Path, folder: &Path| path != folder && path.starts_with(folder);
    if !inside(&named, &chosen) && !inside(&named, &real_folder) {
        return Err(ToolError::BillOutsideFolder);
    }
    if !has_bill_type(&named) {
        return Err(ToolError::BillType);
    }
    let real = joined.canonicalize().map_err(|_| ToolError::BillNotFound)?;
    if !inside(&real, &real_folder) {
        return Err(ToolError::BillOutsideFolder);
    }
    // A link inside the folder may still point to a file of another type.
    if !has_bill_type(&real) {
        return Err(ToolError::BillType);
    }
    if !real.metadata().is_ok_and(|meta| meta.is_file()) {
        return Err(ToolError::BillNotFound);
    }
    Ok(real)
}

/// Whether `path` ends in one of the [`BILL_TYPES`].
fn has_bill_type(path: &Path) -> bool {
    path.extension()
        .and_then(|ending| ending.to_str())
        .is_some_and(|ending| {
            BILL_TYPES
                .iter()
                .any(|kind| ending.eq_ignore_ascii_case(kind))
        })
}

/// `path`, absolute, with `.` and `..` resolved by its names alone,
/// without asking the disk; `None` for a path that isn't absolute or
/// climbs above the root.
fn lexical(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => out.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() || !out.has_root() {
                    return None;
                }
            }
            Component::Normal(name) => out.push(name),
        }
    }
    out.has_root().then_some(out)
}

/// Links the bill the user reviewed, inside the set's user action, after
/// the bills the transaction has. A transaction that is gone, or has the
/// bill already, leaves nothing of the set.
pub(super) fn apply(book: &mut Book, change: &AttachBill) -> Result<(), ChangeError> {
    let stored = match book.transaction(change.transaction) {
        Ok(stored) => stored,
        Err(StoreError::UnknownTransaction(_)) => return Err(ChangeError::OutOfDate),
        Err(error) => return Err(error.into()),
    };
    if stored
        .attachments()
        .iter()
        .any(|attachment| attachment.link == change.link)
    {
        return Err(ChangeError::OutOfDate);
    }
    let mut attachments = stored.attachments().to_vec();
    attachments.push(Attachment {
        link: change.link.clone(),
        title: change.title.clone(),
    });
    book.update_transaction(&stored.with_attachments(attachments))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_resolved_by_its_names_alone() {
        assert_eq!(
            lexical(Path::new("/a/./b/../c")),
            Some(PathBuf::from("/a/c"))
        );
        assert_eq!(lexical(Path::new("/a/../..")), None);
        assert_eq!(lexical(Path::new("/..")), None);
        assert_eq!(lexical(Path::new("a/b")), None);
    }

    #[test]
    fn a_bill_type_is_its_ending_without_case() {
        for name in ["a.pdf", "a.PDF", "a.Png", "a.jpg", "a.JPEG"] {
            assert!(has_bill_type(Path::new(name)), "{name}");
        }
        for name in ["a", "a.pdf.exe", "a.txt", ".pdf", "a.pd"] {
            assert!(!has_bill_type(Path::new(name)), "{name}");
        }
    }
}
