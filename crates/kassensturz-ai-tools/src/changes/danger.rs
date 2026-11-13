// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How much a change set takes away, for the one plain danger line the
//! review shows above a big or erasing set (B-248, attack B1 of the outside
//! security review).
//!
//! One confirm makes every change of a set. The review lists them all, but
//! a long list hides what is lost: a set that empties a hundred notes looks
//! like any other. So the review counts, from the set itself, the texts the
//! user wrote that the set empties or overwrites, how many things it
//! touches, and how many transactions already in the book its new and
//! changed Rules catch; above [`MANY`] or with one lost text it says so in
//! one line and wants a deliberate tick before the confirm button works.
//! The counts come from code, never from the model (§8).

use super::{ChangeSet, FieldValue, ProposedChange};
use kassensturz_core::ai_switch::Field;

/// From how many touched things, or transactions Rules catch, a set counts
/// as big. A safe default (§10): low enough that a set the user can't read
/// through at a glance always gets the line, and far below the 100 items one
/// list may hold.
pub(crate) const MANY: usize = 20;

/// What a set takes away, when it is big or erasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Danger {
    /// The texts the user wrote that the set empties or overwrites: the
    /// payees and notes of transactions, and the names of categories and
    /// accounts.
    pub texts_lost: usize,
    /// How many things the set touches: one for each change, and one for
    /// each transaction a sort with the Rules changes.
    pub touched: usize,
    /// How many transactions already in the book the set's new and changed
    /// Rules catch. A Rule goes on catching what is imported later.
    pub caught: usize,
}

impl ChangeSet {
    /// The danger line's counts, or `None` when the set is small and takes
    /// no text away.
    pub fn danger(&self) -> Option<Danger> {
        let mut danger = Danger {
            texts_lost: 0,
            touched: 0,
            caught: 0,
        };
        for change in self.changes() {
            match change {
                ProposedChange::SortWithRules(sort) => danger.touched += sort.changes.len(),
                _ => danger.touched += 1,
            }
            match change {
                ProposedChange::EditTransaction(edit) => {
                    danger.texts_lost += edit
                        .edits
                        .iter()
                        .filter(|edit| matches!(edit.field, Field::Payee | Field::Note))
                        .filter(|edit| written(&edit.before) && edit.after != edit.before)
                        .count();
                }
                ProposedChange::EditCategory(edit) if edit.before.last() != edit.after.last() => {
                    danger.texts_lost += 1;
                }
                ProposedChange::EditAccount(edit) if edit.before != edit.after => {
                    danger.texts_lost += 1;
                }
                ProposedChange::NewRule(new) => danger.caught += new.catches,
                ProposedChange::EditRule(edit) => danger.caught += edit.catches,
                _ => {}
            }
        }
        let big = danger.touched >= MANY || danger.caught >= MANY;
        (big || danger.texts_lost > 0).then_some(danger)
    }
}

/// Whether a field held text before.
fn written(value: &FieldValue) -> bool {
    matches!(value, FieldValue::Text(Some(text)) if !text.trim().is_empty())
}
