// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboard file (D-441, `docs/REPORT_DESIGNER_M6D.md` section 4.1):
//! one dashboard with the names of what it uses instead of the book's IDs,
//! so it can be given to another book. Writing it is M6d.3's; reading it is
//! in `dashboard_file_read.rs` (M6d.4). The names here are shared by both.

/// The `format` value every dashboard file carries.
pub const DASHBOARD_FILE_FORMAT: &str = "kassensturz-dashboard-file";
/// The version this program writes and the newest it reads.
pub const DASHBOARD_FILE_VERSION: u32 = 1;
/// The largest file, in bytes.
pub const MAX_DASHBOARD_FILE_BYTES: usize = 1 << 20;
/// The most references in one file.
pub const MAX_REFERENCES: usize = 500;
/// The most names in a category's path.
pub const MAX_PATH_NAMES: usize = 16;
/// The longest name in a reference, in characters.
pub const MAX_REFERENCE_NAME_CHARS: usize = 200;
/// The longest `application` text, in characters.
pub const MAX_APPLICATION_CHARS: usize = 100;

/// What a reference key names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Reference {
    /// A category, by the names from the top-level category down.
    Category(Vec<String>),
    /// One of My groups, by its name.
    Group(String),
    /// One of the user's own accounts, by its name.
    Account(String),
}
