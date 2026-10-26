// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What an AI may read from a book, and the check of the numbers in its
//! answer (D-052, D-170, D-186 to D-189).
//!
//! A model reaches the book only through the fixed list of read functions in
//! [`FUNCTIONS`], called with [`call`]: the user's accounts, a balance on a
//! day, the totals per category for a period, a search with a result limit,
//! the categories and the Rules. No function takes SQL, a path or a table
//! name, and none returns a PIN, a key, a bank login, a bank link, an
//! account number, an IBAN or a file (threat T-23). A model that can't call
//! functions gets the small [`overview`] computed in advance instead.
//!
//! Every result is JSON with amounts as exact decimal text (D-065), and
//! every text from the book in it is data, never an instruction (D-171,
//! threat T-21): it is cut to a length limit, stripped of control and
//! direction characters, and each result says it holds data. Each result
//! also carries the [`Figures`] it holds, so that [`check_numbers`] can flag
//! every number in a model's answer that no function produced (threat T-26).
//!
//! A model can also prepare changes, never make them: [`propose`] turns its
//! call of [`PROPOSE_CHANGES`] into a [`ChangeSet`] the user reviews, and
//! only [`ChangeSet::apply`] with the user's [`Confirmed`] changes the book,
//! as one change one undo takes back (D-171, D-198 to D-201).
//!
//! While the user has turned on the debug log, [`read_debug_log`] lets a
//! model read its fixed values, and [`draft_issue`] turns a model's words
//! into an issue draft the user reads before opening it in the browser
//! (D-173).
//!
//! The chat (`kassensturz-ai`, D-047) and the MCP server (`kassensturz-mcp`,
//! D-050) both use this crate, so the two ways in can never differ. Neither
//! this crate nor its callers run while AI is off: the callers check the
//! switch first (D-013), and a build without AI (D-039) leaves the crate
//! empty.

#![forbid(unsafe_code)]

#[cfg(feature = "ai")]
mod changes;
#[cfg(all(feature = "ai", feature = "debug-log"))]
mod debug_log;
#[cfg(feature = "ai")]
mod error;
#[cfg(feature = "ai")]
mod figures;
#[cfg(feature = "ai")]
mod functions;
#[cfg(feature = "ai")]
mod issue;
#[cfg(feature = "ai")]
mod read;
#[cfg(feature = "ai")]
pub mod requests;
#[cfg(feature = "ai")]
mod text;
#[cfg(feature = "ai")]
mod writer;

#[cfg(feature = "ai")]
pub use changes::{
    Categorize, ChangeError, ChangeSet, Confirmed, MAX_CHANGES, NewCategory, NewRule,
    PROPOSE_CHANGES, Proposal, ProposedChange, propose,
};
#[cfg(all(feature = "ai", feature = "debug-log"))]
pub use debug_log::{MAX_LOG_ENTRIES, READ_DEBUG_LOG, read_debug_log};
#[cfg(feature = "ai")]
pub use error::ToolError;
#[cfg(feature = "ai")]
pub use figures::{Figures, Unverified, check_numbers};
#[cfg(feature = "ai")]
pub use functions::{FUNCTIONS, Function, Output, call, overview};
#[cfg(feature = "ai")]
pub use issue::{
    DRAFT_ISSUE, ISSUE_PAGE, IssueContext, IssueDraft, MAX_LINK_BYTES, draft_issue, unique_codes,
};
#[cfg(feature = "ai")]
pub use text::{DATA_NOTE, MAX_TEXT_CHARS};
