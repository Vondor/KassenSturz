// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Category groups ("My groups") and hidden categories: ways of looking at
//! the figures that the book keeps apart from the transactions and the
//! category tree (D-290). Neither ever changes a category, a transaction or a
//! total.

use crate::AccountId;
use crate::AccountKind;
use crate::id::define_id;

define_id!(
    /// Identifies one of the user's category groups within a book.
    GroupId
);

/// What a group's figure means: money spent or money received (D-290). Every
/// category in a group, and every group inside it, is of the group's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GroupKind {
    /// Spending categories.
    Expense,
    /// Income categories.
    Income,
}

impl GroupKind {
    /// The kind of category the group takes.
    pub const fn category_kind(self) -> AccountKind {
        match self {
            Self::Expense => AccountKind::Expense,
            Self::Income => AccountKind::Income,
        }
    }
}

/// One member of a category group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupMember {
    /// A category, which counts with everything inside it (D-291).
    Category(AccountId),
    /// Another group, which counts with all its members.
    Group(GroupId),
}

/// A category group as the book keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryGroup {
    /// The group's ID.
    pub id: GroupId,
    /// The name the user gave it, e.g. "Fixed costs"; never empty, unique
    /// among the groups ignoring capitals.
    pub name: String,
    /// Spending or income.
    pub kind: GroupKind,
    /// The place in the list of groups, counted from 0.
    pub position: u32,
    /// The members in the order the user put them, each once.
    pub members: Vec<GroupMember>,
}
