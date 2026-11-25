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
use std::time::UNIX_EPOCH;

use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_core::{Attachment, Date, Transaction, TransactionId};
use kassensturz_store::{Book, StoreError};
use serde_json::Value;

use super::transactions::free_text;
use super::{ChangeError, Folders, Known, Object};
use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};
use crate::text::for_model;

/// The list of `propose_changes` that holds the bills.
pub(super) const LIST: &str = "attach_bills";

/// The most bills one set may attach, so the user can still review them.
pub(super) const MAX_BILLS: usize = 100;

/// The most files [`BILL_FOLDER`] or [`crate::INBOX`] lists in one call.
pub(crate) const MAX_LISTED_BILLS: usize = 200;

/// The file types a bill may have, by the end of its name, compared
/// without case (D-739, D-990).
pub const BILL_TYPES: [&str; 6] = ["pdf", "png", "jpg", "jpeg", "tif", "tiff"];

/// The function that lists the bill folder (D-739, D-990). Offered only
/// while a bill folder is chosen and *Transactions › Attach bills* is on,
/// so a path never goes to a model the user didn't allow.
pub const BILL_FOLDER: Function = Function {
    name: "bill_folder",
    description: "Lists the bills waiting in the user's bill folder: the folder's path, the \
        allowed file types (PDF, PNG, JPEG, TIFF), and each regular bill file's name, size and \
        date directly inside that folder. Use read_bill to inspect a bill, find the transaction \
        it was paid with by its invoice or order number (search_transactions with reference), \
        and link it with attach_bills in propose_changes.",
    parameters: r#"{"type":"object","properties":{},"additionalProperties":false}"#,
};

/// What [`BILL_FOLDER`] answers for `folder`: its path as the user chose it,
/// [`BILL_TYPES`], and a bounded non-recursive listing of plain bill files
/// right inside it, sorted deterministically by name (D-990, T-21, T-62,
/// T-72). Subdirectories, symlinks, special files and other extensions are
/// skipped; filenames are cleaned as untrusted data.
pub fn bill_folder_answer(folder: &Path, arguments: &Value) -> Result<Output, ToolError> {
    check_empty_arguments(arguments)?;
    list_bill_dir(folder, || ToolError::NoBillFolder)
}

/// [`bill_folder_answer`], gated on [`Permission::AttachBills`] and a
/// configured bill folder in `folders` (D-739, D-990).
pub fn bill_folder_answer_allowed(
    folders: &Folders,
    arguments: &Value,
    permissions: Permissions,
) -> Result<Output, ToolError> {
    if !permissions.allows(Permission::AttachBills) {
        return Err(ToolError::NotAllowed {
            argument: BILL_FOLDER.name,
        });
    }
    let folder = folders.bills.as_deref().ok_or(ToolError::NoBillFolder)?;
    bill_folder_answer(folder, arguments)
}

/// Checks that a zero-argument tool received `null` or `{}`.
pub(super) fn check_empty_arguments(arguments: &Value) -> Result<(), ToolError> {
    match arguments {
        Value::Null => Ok(()),
        Value::Object(values) if values.is_empty() => Ok(()),
        _ => Err(ToolError::BadArgument {
            argument: "arguments",
        }),
    }
}

/// Shared non-recursive, bounded, sorted listing of plain bill files right
/// inside `folder` (used by [`bill_folder_answer`] and
/// [`super::filing::inbox_answer`]).
///
/// Only regular files directly in `folder` whose extension is in
/// [`BILL_TYPES`] are listed: subdirectories, symlinks (whether pointing
/// inside or outside the folder), special files and other extensions are
/// skipped. Every returned filename is cleaned with [`for_model`] (T-21).
pub(super) fn list_bill_dir(
    folder: &Path,
    missing: impl Fn() -> ToolError,
) -> Result<Output, ToolError> {
    let folder_str = folder.to_str().ok_or_else(&missing)?.to_owned();
    let real_folder = folder.canonicalize().map_err(|_| missing())?;
    let entries = std::fs::read_dir(&real_folder).map_err(|_| missing())?;
    let mut files: Vec<(String, String, u64, Option<String>)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            // `file_type` and `symlink_metadata` never follow symlinks, so
            // links (including escaping symlinks), subdirectories and special
            // files are skipped.
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                return None;
            }
            let raw_name = entry.file_name().to_str()?.to_owned();
            if !has_bill_type(Path::new(&raw_name)) {
                return None;
            }
            let meta = std::fs::symlink_metadata(entry.path()).ok()?;
            if !meta.is_file() {
                return None;
            }
            let modified = meta.modified().ok().and_then(|time| {
                let since = time.duration_since(UNIX_EPOCH).ok()?;
                let seconds = i64::try_from(since.as_secs()).ok()?;
                let stamp = kassensturz_core::Timestamp::from_second(seconds).ok()?;
                Some(stamp.to_string())
            });
            let clean_name = for_model(&raw_name);
            Some((clean_name, raw_name, meta.len(), modified))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let total = files.len();
    let truncated = total > MAX_LISTED_BILLS;
    files.truncate(MAX_LISTED_BILLS);
    let listed: Vec<Value> = files
        .into_iter()
        .map(|(name, _, size, modified)| {
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
        "folder": folder_str,
        "file_types": BILL_TYPES,
        "files": listed,
        "truncated": truncated,
    });
    if truncated {
        answer["files_left_out"] = Value::from(total - MAX_LISTED_BILLS);
    }
    Ok(Output::new(answer, figures))
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
    /// The transaction as the user reviewed it: linking fails if it changed
    /// since (B-338).
    pub(super) reviewed: Transaction,
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
            reviewed: stored.clone(),
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

/// Whether `path` ends in one of the [`BILL_TYPES`], compared without case.
pub(crate) fn has_bill_type(path: &Path) -> bool {
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
        for name in [
            "a.pdf", "a.PDF", "a.Png", "a.jpg", "a.JPEG", "a.tif", "a.TIFF",
        ] {
            assert!(has_bill_type(Path::new(name)), "{name}");
        }
        for name in ["a", "a.pdf.exe", "a.txt", ".pdf", "a.pd", "a.tif.sh"] {
            assert!(!has_bill_type(Path::new(name)), "{name}");
        }
    }
}
