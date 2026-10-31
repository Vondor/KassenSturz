// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's corrections to a change set before they confirm it (M6b.8,
//! D-293, D-297, threat T-44).
//!
//! A set keeps the model's call as it came. An edit changes a few fields of
//! one item of that call, and leaving a change out drops its item; the call
//! as the user leaves it then goes through [`super::build`] again, the same
//! checks as the model's call, with the permissions of the moment. So an
//! edit can never turn a change into another kind, add a kind the user
//! doesn't allow, or skip a check. The set stays the model's: it is applied
//! as the one `apply_ai_changes` entry, and nothing of the edit goes back to
//! the model.

use std::collections::BTreeMap;

use kassensturz_core::ai_switch::Permissions;
use kassensturz_core::{Date, ErrorReport, Report};
use kassensturz_store::Book;
use serde_json::{Map, Value};

use super::{Built, ChangeSet, NewCategory, ProposedChange, build, views};
use crate::error::ToolError;

/// The lists of a call in the order [`super::build`] turns their items into
/// changes, one change for each item.
const ORDER: [&str; 16] = [
    "new_categories",
    "edit_categories",
    "new_rules",
    "edit_rules",
    "delete_rules",
    "move_rules",
    "categorize",
    "new_accounts",
    "edit_accounts",
    "delete_categories",
    "edit_transactions",
    "new_transactions",
    "delete_transactions",
    "new_views",
    "new_budgets",
    "edit_budgets",
];

/// The fields the user may change in an item of each list (D-293). Every
/// change can be left out; only these can be edited.
const EDITABLE: [(&str, &[&str]); 4] = [
    ("new_categories", &["name", "parent_id"]),
    (
        "new_rules",
        &["name", "match", "conditions", "category_id", "new_category"],
    ),
    ("categorize", &["category_id", "new_category"]),
    ("new_views", &["title", "chart", "period"]),
];

/// Where a change stands in the model's call: which list, and which item of
/// it. It stays the same while the user edits the set, so the review page
/// names a change by it, e.g. `new_rules/0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot {
    list: &'static str,
    index: usize,
}

impl Slot {
    /// The slot written as [`Slot`]'s `Display` writes it, if it is one.
    pub fn parse(text: &str) -> Option<Self> {
        let (list, index) = text.split_once('/')?;
        let list = ORDER.into_iter().find(|known| *known == list)?;
        Some(Self {
            list,
            index: index.parse().ok()?,
        })
    }

    /// The fields of this change the user may edit; none for a change that
    /// can only be left out.
    pub fn editable(self) -> &'static [&'static str] {
        EDITABLE
            .iter()
            .find(|(list, _)| *list == self.list)
            .map_or(&[], |(_, fields)| fields)
    }
}

impl core::fmt::Display for Slot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}/{}", self.list, self.index)
    }
}

/// The model's call and what the user changed in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Call {
    /// The call as the model made it.
    ai: Value,
    /// The user's version of single items: `None` leaves the item out.
    edits: BTreeMap<Slot, Option<Value>>,
    /// Every change as the model prepared it, for a change the user left
    /// out, which the page still lists.
    ai_changes: Vec<(Slot, ProposedChange)>,
    /// The day the set was prepared on, which budgets start with (D-394):
    /// the user's edits are checked on the same day.
    today: Option<Date>,
}

impl Call {
    /// The call of a fresh proposal, and the slot of each of its changes.
    pub(super) fn first(
        arguments: &Value,
        changes: &[ProposedChange],
        today: Option<Date>,
    ) -> (Vec<Slot>, Self) {
        let call = Self {
            today,
            ai: arguments.clone(),
            edits: BTreeMap::new(),
            ai_changes: Vec::new(),
        };
        let slots = call.slots();
        debug_assert!(
            slots
                .iter()
                .zip(changes)
                .all(|(slot, change)| slot.list == list_of(change)),
            "a change and its slot are of one list"
        );
        let ai_changes = slots.iter().copied().zip(changes.iter().cloned()).collect();
        (slots, Self { ai_changes, ..call })
    }

    /// The items of one list of the model's call.
    fn ai_items(&self, list: &str) -> &[Value] {
        self.ai
            .get(list)
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice)
    }

    fn ai_item(&self, slot: Slot) -> Option<&Value> {
        self.ai_items(slot.list).get(slot.index)
    }

    /// The slots of the items the user keeps, in the order of the changes.
    fn slots(&self) -> Vec<Slot> {
        ORDER
            .into_iter()
            .flat_map(|list| (0..self.ai_items(list).len()).map(move |index| Slot { list, index }))
            .filter(|slot| self.edits.get(slot) != Some(&None))
            .collect()
    }

    /// The call as the user leaves it.
    fn as_edited(&self) -> Value {
        let mut call = Map::new();
        for list in ORDER {
            let items: Vec<Value> = self
                .ai_items(list)
                .iter()
                .enumerate()
                .filter_map(
                    |(index, item)| match self.edits.get(&Slot { list, index }) {
                        Some(edited) => edited.clone(),
                        None => Some(item.clone()),
                    },
                )
                .collect();
            if !items.is_empty() {
                call.insert(list.to_owned(), Value::Array(items));
            }
        }
        Value::Object(call)
    }
}

/// Why an edit of a change set wasn't kept. The set stays as it was before
/// the edit.
#[derive(Debug)]
pub enum EditError {
    /// The change can't be edited this way: it isn't in the set, it can
    /// only be left out, or the edit names a field it doesn't have.
    NotEditable,
    /// A name, title or text is empty, too long, or holds characters that
    /// hide text.
    Text,
    /// The name is already used.
    NameTaken,
    /// Another change of the set uses the one the user left out, e.g. a Rule
    /// that books to a new category.
    InUse,
    /// The set would hold a kind of change the user doesn't allow now.
    NotAllowed,
    /// The checks refused the edit for another reason, e.g. a parent of the
    /// other kind or a chart that doesn't suit its figures.
    Refused(ToolError),
}

impl From<ToolError> for EditError {
    fn from(error: ToolError) -> Self {
        match error {
            ToolError::BadArgument {
                argument: "name" | "title" | "text",
            } => Self::Text,
            ToolError::NameTaken => Self::NameTaken,
            // The user picks a new category from the set, so a key that
            // isn't there any more was left out.
            ToolError::BadArgument {
                argument: "new_category" | "new_parent",
            } => Self::InUse,
            ToolError::NotAllowed { .. } => Self::NotAllowed,
            other => Self::Refused(other),
        }
    }
}

impl core::fmt::Display for EditError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.report().code)
    }
}

impl std::error::Error for EditError {}

impl Report for EditError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::NotEditable => ErrorReport::new("ai-changes.not-editable"),
            Self::Text => ErrorReport::new("ai-changes.edit-text"),
            Self::NameTaken => ErrorReport::new("ai-changes.edit-name-taken"),
            Self::InUse => ErrorReport::new("ai-changes.edit-in-use"),
            Self::NotAllowed => ErrorReport::new("ai-changes.not-allowed"),
            Self::Refused(error) => {
                ErrorReport::new("ai-changes.edit-refused").caused_by(error.report())
            }
        }
    }
}

impl ChangeSet {
    /// Where each change stands in the model's call, in the order of
    /// [`ChangeSet::changes`].
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Whether the user changed this change: the page marks it "changed by
    /// you" and offers the model's version back.
    pub fn edited(&self, slot: Slot) -> bool {
        matches!(self.call.edits.get(&slot), Some(Some(_)))
    }

    /// Whether the user changed or left out any change of the set, which
    /// the chat hears as a fixed line once it is confirmed (D-298).
    pub fn corrected(&self) -> bool {
        !self.call.edits.is_empty()
    }

    /// The changes the user left out, as the model prepared them.
    pub fn left_out(&self) -> impl Iterator<Item = (Slot, &ProposedChange)> {
        self.call
            .ai_changes
            .iter()
            .filter(|(slot, _)| self.call.edits.get(slot) == Some(&None))
            .map(|(slot, change)| (*slot, change))
    }

    /// The item of the call a change comes from, as it stands: what the
    /// page's edit form starts from. `None` for a change left out.
    pub fn item(&self, slot: Slot) -> Option<&Value> {
        match self.call.edits.get(&slot) {
            Some(edited) => edited.as_ref(),
            None => self.call.ai_item(slot),
        }
    }

    /// The new categories of the set with the keys other changes name them
    /// by, for the page's choice of a category.
    pub fn new_categories(&self) -> impl Iterator<Item = (&str, &NewCategory)> {
        self.slots
            .iter()
            .zip(&self.changes)
            .filter_map(|(slot, change)| match change {
                ProposedChange::NewCategory(new) => self
                    .item(*slot)
                    .and_then(|item| item.get("key"))
                    .and_then(Value::as_str)
                    .map(|key| (key.trim(), new)),
                _ => None,
            })
    }

    /// The set with the fields `fields` of one change set as the user typed
    /// them, in the call's own words (`name`, `parent_id`, `category_id` and
    /// so on); a null removes a field. It is checked like the model's call,
    /// against `book` and `allowed` as they are now.
    pub fn edit(
        &self,
        book: &Book,
        slot: Slot,
        fields: &Value,
        allowed: Permissions,
    ) -> Result<Self, EditError> {
        let editable = slot.editable();
        let (Some(Value::Object(current)), Value::Object(fields)) = (self.item(slot), fields)
        else {
            return Err(EditError::NotEditable);
        };
        if fields.is_empty() || fields.keys().any(|key| !editable.contains(&key.as_str())) {
            return Err(EditError::NotEditable);
        }
        let mut item = current.clone();
        // A category is either one of the book's or a new one of the set.
        if fields.contains_key("category_id") || fields.contains_key("new_category") {
            item.remove("category_id");
            item.remove("new_category");
        }
        for (key, value) in fields {
            if value.is_null() {
                item.remove(key);
            } else {
                item.insert(key.clone(), value.clone());
            }
        }
        if fields.contains_key("chart") {
            views::keep_fitting(&mut item);
        }
        let item = Value::Object(item);
        let mut call = self.call.clone();
        if self.call.ai_item(slot) == Some(&item) {
            call.edits.remove(&slot);
        } else {
            call.edits.insert(slot, Some(item));
        }
        Self::checked(book, call, allowed)
    }

    /// [`ChangeSet::edit`] with the fields as JSON text, as the review page
    /// sends them; text that isn't a JSON object edits nothing.
    pub fn edit_text(
        &self,
        book: &Book,
        slot: Slot,
        fields: &str,
        allowed: Permissions,
    ) -> Result<Self, EditError> {
        let fields: Value = serde_json::from_str(fields).map_err(|_| EditError::NotEditable)?;
        self.edit(book, slot, &fields, allowed)
    }

    /// The set without one change.
    pub fn leave_out(
        &self,
        book: &Book,
        slot: Slot,
        allowed: Permissions,
    ) -> Result<Self, EditError> {
        if !self.slots.contains(&slot) {
            return Err(EditError::NotEditable);
        }
        let mut call = self.call.clone();
        call.edits.insert(slot, None);
        Self::checked(book, call, allowed)
    }

    /// The set with one change as the model prepared it again, after the
    /// user edited it or left it out.
    pub fn back_to_ai(
        &self,
        book: &Book,
        slot: Slot,
        allowed: Permissions,
    ) -> Result<Self, EditError> {
        let mut call = self.call.clone();
        if call.edits.remove(&slot).is_none() {
            return Err(EditError::NotEditable);
        }
        Self::checked(book, call, allowed)
    }

    /// The set the user's version of `call` makes, through the same checks
    /// as the model's call.
    fn checked(book: &Book, call: Call, allowed: Permissions) -> Result<Self, EditError> {
        let Built {
            changes,
            rule_order,
        } = build(book, &call.as_edited(), allowed, call.today)?;
        let slots = call.slots();
        debug_assert!(
            slots
                .iter()
                .zip(&changes)
                .all(|(slot, change)| slot.list == list_of(change)),
            "a change and its slot are of one list"
        );
        Ok(Self {
            changes,
            rule_order,
            slots,
            call,
        })
    }
}

/// The list of a call a change comes from.
pub(super) fn list_of(change: &ProposedChange) -> &'static str {
    match change {
        ProposedChange::NewCategory(_) => "new_categories",
        ProposedChange::EditCategory(_) => "edit_categories",
        ProposedChange::NewRule(_) => "new_rules",
        ProposedChange::EditRule(_) => "edit_rules",
        ProposedChange::DeleteRule(_) => "delete_rules",
        ProposedChange::MoveRule(_) => "move_rules",
        ProposedChange::Categorize(_) => "categorize",
        ProposedChange::NewAccount(_) => "new_accounts",
        ProposedChange::EditAccount(_) => "edit_accounts",
        ProposedChange::DeleteCategory(_) => "delete_categories",
        ProposedChange::EditTransaction(_) => "edit_transactions",
        ProposedChange::NewTransaction(_) => "new_transactions",
        ProposedChange::DeleteTransaction(_) => "delete_transactions",
        ProposedChange::NewView(_) => "new_views",
        ProposedChange::NewBudget(_) => "new_budgets",
        ProposedChange::EditBudget(_) => "edit_budgets",
    }
}
