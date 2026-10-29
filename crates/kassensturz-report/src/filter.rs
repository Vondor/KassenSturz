// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a dataset leaves out.

use kassensturz_core::AccountId;

/// Narrows a dataset to some accounts and some categories. `None` means no
/// narrowing; `Some` with an empty list means nothing is chosen, so the
/// dataset is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// Only transactions with a part booked to one of these accounts of the
    /// user's. A transaction counts as a whole: if it books money out of
    /// two accounts and only one is chosen, all its categories still count.
    pub accounts: Option<Vec<AccountId>>,
    /// Only parts booked to these categories or their subcategories. Money
    /// not sorted into a category yet is left out, since it is in none.
    pub categories: Option<Vec<AccountId>>,
}
