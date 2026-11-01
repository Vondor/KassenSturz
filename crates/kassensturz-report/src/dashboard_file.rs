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

/// What a reference key of a dashboard file names (D-432): never an ID,
/// always names, looked up in the book that reads the file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Reference {
    /// A category by its names from the top category down.
    Category(Vec<String>),
    /// One of My groups by its name.
    Group(String),
    /// One of the user's own accounts by its name.
    Account(String),
}
