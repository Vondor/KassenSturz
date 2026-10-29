// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Category groups ("My groups") and hidden categories (D-290; layout 21).
//!
//! Both are kept in the book apart from the transactions and the category
//! tree (Q-221): they refer to categories by ID, and nothing here ever writes
//! to a transaction or a category. The book checks what keeps a group's
//! figure meaningful (members of the group's kind, no loops, at most four
//! levels, the size limits); how groups and hidden categories count is the
//! reporting layer's job (D-291).

use std::collections::{HashMap, HashSet};

use kassensturz_core::{AccountId, CategoryGroup, GroupId, GroupKind, GroupMember};
use rusqlite::{OptionalExtension, params};

use crate::Action;
use crate::Book;
use crate::error::StoreError;

/// The most groups a book keeps (D-290).
const MAX_GROUPS: usize = 200;
/// The most members a group keeps (D-290).
const MAX_MEMBERS: usize = 500;
/// The most levels of groups inside groups: a group is one level, a group
/// inside it a second, and so on (D-290).
const MAX_LEVELS: usize = 4;
/// The longest name, in characters, as for views (D-281).
const MAX_NAME_CHARS: usize = 200;

fn kind_to_text(kind: GroupKind) -> &'static str {
    match kind {
        GroupKind::Expense => "expense",
        GroupKind::Income => "income",
    }
}

fn kind_from_text(text: &str) -> GroupKind {
    // The table's CHECK allows nothing else.
    if text == "income" {
        GroupKind::Income
    } else {
        GroupKind::Expense
    }
}

/// A group's name without spaces around it, if it is one the book keeps.
fn checked_name(name: &str) -> Result<String, StoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(StoreError::GroupNameTooLong {
            max_chars: MAX_NAME_CHARS,
        });
    }
    Ok(name.to_owned())
}

/// The members in their order, each once: a member given twice keeps its
/// first place.
fn distinct(members: &[GroupMember]) -> Vec<GroupMember> {
    let mut seen = HashSet::new();
    members
        .iter()
        .copied()
        .filter(|member| seen.insert(*member))
        .collect()
}

/// Which groups each group holds, for the checks for loops and levels.
type Nesting = HashMap<GroupId, Vec<GroupId>>;

/// How many levels deep `group` goes (1 if it holds no groups), or `None`
/// if it is inside itself somewhere below. `open` holds the groups on the
/// way down, `levels` the groups already measured.
fn levels_of(
    group: GroupId,
    nesting: &Nesting,
    open: &mut HashSet<GroupId>,
    levels: &mut HashMap<GroupId, usize>,
) -> Option<usize> {
    if let Some(&known) = levels.get(&group) {
        return Some(known);
    }
    if !open.insert(group) {
        return None;
    }
    let mut deepest = 0;
    for &inner in nesting.get(&group).map_or(&[][..], Vec::as_slice) {
        deepest = deepest.max(levels_of(inner, nesting, open, levels)?);
    }
    open.remove(&group);
    levels.insert(group, deepest + 1);
    Some(deepest + 1)
}

/// The first group, in the order given, that is inside itself, or the
/// deepest level any group reaches.
fn check_nesting(order: &[GroupId], nesting: &Nesting) -> Result<usize, GroupId> {
    let mut levels = HashMap::new();
    let mut deepest = 0;
    for &group in order {
        let mut open = HashSet::new();
        let level = levels_of(group, nesting, &mut open, &mut levels).ok_or(group)?;
        deepest = deepest.max(level);
    }
    Ok(deepest)
}

fn member_columns(member: GroupMember) -> (Option<[u8; 16]>, Option<[u8; 16]>) {
    match member {
        GroupMember::Category(category) => (Some(category.to_bytes()), None),
        GroupMember::Group(group) => (None, Some(group.to_bytes())),
    }
}

impl Book {
    /// The category groups in the order of their list, each with its members
    /// in their order.
    pub fn category_groups(&self) -> Result<Vec<CategoryGroup>, StoreError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT id, name, kind, position FROM report_groups ORDER BY position",
        )?;
        let mut groups = statement
            .query_map([], |row| {
                Ok(CategoryGroup {
                    id: GroupId::from_bytes(row.get(0)?),
                    name: row.get(1)?,
                    kind: kind_from_text(&row.get::<_, String>(2)?),
                    position: row.get(3)?,
                    members: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let place: HashMap<GroupId, usize> = groups
            .iter()
            .enumerate()
            .map(|(index, group)| (group.id, index))
            .collect();
        let mut statement = self.conn.prepare_cached(
            "SELECT group_id, category_id, member_group_id FROM report_group_members
             ORDER BY group_id, position, id",
        )?;
        let members = statement
            .query_map([], |row| {
                let group = GroupId::from_bytes(row.get(0)?);
                let category: Option<[u8; 16]> = row.get(1)?;
                let member = match category {
                    Some(category) => GroupMember::Category(AccountId::from_bytes(category)),
                    // The table's CHECK makes sure one of the two is set.
                    None => GroupMember::Group(GroupId::from_bytes(row.get(2)?)),
                };
                Ok((group, member))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (group, member) in members {
            // The foreign key makes sure the group is there.
            if let Some(&index) = place.get(&group) {
                groups[index].members.push(member);
            }
        }
        Ok(groups)
    }

    /// The category group `id` ([`StoreError::UnknownGroup`] if there is
    /// none).
    pub fn category_group(&self, id: GroupId) -> Result<CategoryGroup, StoreError> {
        self.category_groups()?
            .into_iter()
            .find(|group| group.id == id)
            .ok_or(StoreError::UnknownGroup(id))
    }

    /// Makes a new category group at the end of the list; the user action
    /// [`Action::AddGroup`].
    ///
    /// The name is kept without spaces around it, must not be empty
    /// ([`StoreError::EmptyName`]), has at most 200 characters
    /// ([`StoreError::GroupNameTooLong`]) and may not be the name of another
    /// group or of a category, ignoring capitals
    /// ([`StoreError::GroupNameTaken`]), so a formula's name stays clear
    /// (D-292). A book keeps at most 200 groups
    /// ([`StoreError::TooManyGroups`]). The members follow the rules of
    /// [`Book::edit_category_group`].
    pub fn add_category_group(
        &mut self,
        name: &str,
        kind: GroupKind,
        members: &[GroupMember],
    ) -> Result<GroupId, StoreError> {
        let id = GroupId::generate();
        let name = checked_name(name)?;
        self.as_one_action(Action::AddGroup, |book| {
            let count = book.group_count()?;
            if count as usize >= MAX_GROUPS {
                return Err(StoreError::TooManyGroups { max: MAX_GROUPS });
            }
            book.check_group_name(None, &name, true)?;
            let members = book.checked_members(id, &name, kind, members)?;
            book.conn.execute(
                "INSERT INTO report_groups (id, name, kind, position) VALUES (?1, ?2, ?3, ?4)",
                params![id.to_bytes(), name, kind_to_text(kind), count],
            )?;
            book.write_members(id, &members)?;
            book.check_nesting_of(&name)?;
            Ok(id)
        })
    }

    /// Gives the category group `id` a new name and new members, in the
    /// order given; the user action [`Action::EditGroup`]. Its kind stays.
    ///
    /// The name follows the rules of [`Book::add_category_group`]; a name
    /// that differs from the old one only in capitals is not compared with
    /// the categories again, so a group keeps its name when a category gets
    /// the same one later.
    ///
    /// Each member is in the group once (a member given twice keeps its
    /// first place), and a group holds at most 500
    /// ([`StoreError::TooManyGroupMembers`]). A member category must be in
    /// the book ([`StoreError::UnknownAccount`]), a category and not an
    /// account ([`StoreError::NotACategory`]); a member group must be in the
    /// book ([`StoreError::UnknownGroup`]). Each must be of the group's kind
    /// ([`StoreError::GroupWrongKind`]). A group can't end up inside itself
    /// ([`StoreError::GroupLoop`]), and groups go at most four levels deep
    /// ([`StoreError::GroupTooDeep`]). Nothing that stays the same is
    /// recorded.
    pub fn edit_category_group(
        &mut self,
        id: GroupId,
        name: &str,
        members: &[GroupMember],
    ) -> Result<(), StoreError> {
        let old = self.category_group(id)?;
        let name = checked_name(name)?;
        self.as_one_action(Action::EditGroup, |book| {
            let renamed = name.to_lowercase() != old.name.to_lowercase();
            book.check_group_name(Some(id), &name, renamed)?;
            let members = book.checked_members(id, &name, old.kind, members)?;
            book.conn.execute(
                "UPDATE report_groups SET name = ?2 WHERE id = ?1 AND name <> ?2",
                params![id.to_bytes(), name],
            )?;
            book.write_members(id, &members)?;
            book.check_nesting_of(&name)
        })
    }

    /// Moves the category group `id` to `position` in the list (counted
    /// from 0; a place past the end means the end); the user action
    /// [`Action::MoveGroup`]. The others keep their order and the positions
    /// stay numbered 0, 1, 2, ….
    pub fn move_category_group(&mut self, id: GroupId, position: u32) -> Result<(), StoreError> {
        self.category_group(id)?;
        self.as_one_action(Action::MoveGroup, |book| {
            let mut ids = book.group_ids()?;
            ids.retain(|&other| other != id);
            let at = usize::try_from(position)
                .unwrap_or(usize::MAX)
                .min(ids.len());
            ids.insert(at, id);
            book.number_groups(&ids)
        })
    }

    /// Removes the category group `id`; the user action
    /// [`Action::RemoveGroup`]. Its members go with it, and so does its place
    /// in every group that holds it; the categories stay as they are. The
    /// groups behind it move up by one.
    pub fn remove_category_group(&mut self, id: GroupId) -> Result<(), StoreError> {
        self.category_group(id)?;
        self.as_one_action(Action::RemoveGroup, |book| {
            book.conn
                .execute("DELETE FROM report_groups WHERE id = ?1", [id.to_bytes()])?;
            let ids = book.group_ids()?;
            book.number_groups(&ids)
        })
    }

    /// Puts back category groups with the IDs they had, e.g. from a complete
    /// export (D-029): in the order given, each with its members in their
    /// order. The checks of [`Book::edit_category_group`] apply, except that
    /// a name may be a category's, as it may in the book the groups come
    /// from. A member group may come later in the list.
    ///
    /// # Panics
    ///
    /// Outside [`Book::fill_new`].
    pub fn restore_category_groups(&mut self, groups: &[CategoryGroup]) -> Result<(), StoreError> {
        assert!(
            self.filling,
            "only a new book being filled takes groups this way"
        );
        self.as_one_action(Action::AddGroup, |book| {
            let count = book.group_count()?;
            if count as usize + groups.len() > MAX_GROUPS {
                return Err(StoreError::TooManyGroups { max: MAX_GROUPS });
            }
            let mut names = Vec::with_capacity(groups.len());
            for (position, group) in (count..).zip(groups) {
                let name = checked_name(&group.name)?;
                book.check_group_name(None, &name, false)?;
                book.conn.execute(
                    "INSERT INTO report_groups (id, name, kind, position) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        group.id.to_bytes(),
                        name,
                        kind_to_text(group.kind),
                        position
                    ],
                )?;
                names.push(name);
            }
            for (group, name) in groups.iter().zip(&names) {
                let members = book.checked_members(group.id, name, group.kind, &group.members)?;
                book.write_members(group.id, &members)?;
            }
            book.check_nesting_of("")
        })
    }

    /// The categories hidden from charts (D-291), in the order of their IDs.
    pub fn hidden_categories(&self) -> Result<Vec<AccountId>, StoreError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT category_id FROM report_hidden ORDER BY category_id")?;
        let hidden = statement
            .query_map([], |row| Ok(AccountId::from_bytes(row.get(0)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(hidden)
    }

    /// Hides exactly these categories from charts, and shows every other one
    /// again; the user action [`Action::SetHiddenCategories`]. Each must be
    /// in the book ([`StoreError::UnknownAccount`]) and a category, not an
    /// account ([`StoreError::NotACategory`]); one given twice is hidden
    /// once. Hiding never changes a group, a formula or a total (Q-225,
    /// D-291). Categories that stay hidden or shown are not recorded.
    pub fn set_hidden_categories(&mut self, categories: &[AccountId]) -> Result<(), StoreError> {
        let wanted: HashSet<AccountId> = categories.iter().copied().collect();
        for &category in &wanted {
            if !self.account(category)?.kind.is_category() {
                return Err(StoreError::NotACategory(category));
            }
        }
        self.as_one_action(Action::SetHiddenCategories, |book| {
            let hidden: HashSet<AccountId> = book.hidden_categories()?.into_iter().collect();
            for gone in hidden.difference(&wanted) {
                book.conn.execute(
                    "DELETE FROM report_hidden WHERE category_id = ?1",
                    [gone.to_bytes()],
                )?;
            }
            for new in wanted.difference(&hidden) {
                book.conn.execute(
                    "INSERT INTO report_hidden (category_id) VALUES (?1)",
                    [new.to_bytes()],
                )?;
            }
            Ok(())
        })
    }

    /// Refuses a name another group has, ignoring capitals, and, if
    /// `with_categories`, one a category has.
    fn check_group_name(
        &self,
        own: Option<GroupId>,
        name: &str,
        with_categories: bool,
    ) -> Result<(), StoreError> {
        let wanted = name.to_lowercase();
        let mut statement = self
            .conn
            .prepare_cached("SELECT id, name FROM report_groups")?;
        let groups = statement
            .query_map([], |row| {
                Ok((GroupId::from_bytes(row.get(0)?), row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let taken = groups
            .iter()
            .any(|(id, other)| Some(*id) != own && other.to_lowercase() == wanted)
            || with_categories
                && self.all_accounts()?.iter().any(|account| {
                    account.kind.is_category() && account.name.to_lowercase() == wanted
                });
        if taken {
            return Err(StoreError::GroupNameTaken {
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    /// The members of the group `own` (called `name`, of `kind`), each once
    /// and each checked; see [`Book::edit_category_group`].
    fn checked_members(
        &self,
        own: GroupId,
        name: &str,
        kind: GroupKind,
        members: &[GroupMember],
    ) -> Result<Vec<GroupMember>, StoreError> {
        let members = distinct(members);
        if members.len() > MAX_MEMBERS {
            return Err(StoreError::TooManyGroupMembers { max: MAX_MEMBERS });
        }
        for &member in &members {
            let (member_name, member_kind) = match member {
                GroupMember::Category(category) => {
                    let account = self.account(category)?;
                    if !account.kind.is_category() {
                        return Err(StoreError::NotACategory(category));
                    }
                    (account.name, account.kind)
                }
                GroupMember::Group(group) if group == own => {
                    return Err(StoreError::GroupLoop {
                        name: name.to_owned(),
                    });
                }
                GroupMember::Group(group) => {
                    let (member_name, member_kind): (String, String) = self
                        .conn
                        .query_row(
                            "SELECT name, kind FROM report_groups WHERE id = ?1",
                            [group.to_bytes()],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()?
                        .ok_or(StoreError::UnknownGroup(group))?;
                    (member_name, kind_from_text(&member_kind).category_kind())
                }
            };
            if member_kind != kind.category_kind() {
                return Err(StoreError::GroupWrongKind { name: member_name });
            }
        }
        Ok(members)
    }

    /// Makes the member rows of `group` match `members`, numbered 0, 1, 2,
    /// … in their order. A member that stays keeps its row, and a row that
    /// already has its place is not touched, so it is not recorded.
    fn write_members(&self, group: GroupId, members: &[GroupMember]) -> Result<(), StoreError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT id, category_id, member_group_id FROM report_group_members
             WHERE group_id = ?1",
        )?;
        let mut rows: HashMap<GroupMember, [u8; 16]> = statement
            .query_map([group.to_bytes()], |row| {
                let category: Option<[u8; 16]> = row.get(1)?;
                let member = match category {
                    Some(category) => GroupMember::Category(AccountId::from_bytes(category)),
                    None => GroupMember::Group(GroupId::from_bytes(row.get(2)?)),
                };
                Ok((member, row.get(0)?))
            })?
            .collect::<Result<_, _>>()?;
        for (position, &member) in (0_u32..).zip(members) {
            if let Some(row) = rows.remove(&member) {
                self.conn.execute(
                    "UPDATE report_group_members SET position = ?2
                     WHERE id = ?1 AND position <> ?2",
                    params![row, position],
                )?;
            } else {
                let (category, member_group) = member_columns(member);
                self.conn.execute(
                    "INSERT INTO report_group_members
                         (id, group_id, category_id, member_group_id, position)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        uuid::Uuid::now_v7().into_bytes(),
                        group.to_bytes(),
                        category,
                        member_group,
                        position
                    ],
                )?;
            }
        }
        for row in rows.into_values() {
            self.conn
                .execute("DELETE FROM report_group_members WHERE id = ?1", [row])?;
        }
        Ok(())
    }

    /// Checks the groups as they are now for loops and levels. A loop is
    /// named after `changed`, the group just written, if one was; otherwise
    /// after a group on the loop.
    fn check_nesting_of(&self, changed: &str) -> Result<(), StoreError> {
        let groups = self.category_groups()?;
        let nesting: Nesting = groups
            .iter()
            .map(|group| {
                let inner = group
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        GroupMember::Group(inner) => Some(*inner),
                        GroupMember::Category(_) => None,
                    })
                    .collect();
                (group.id, inner)
            })
            .collect();
        let order: Vec<GroupId> = groups.iter().map(|group| group.id).collect();
        match check_nesting(&order, &nesting) {
            Ok(levels) if levels > MAX_LEVELS => Err(StoreError::GroupTooDeep {
                max_levels: MAX_LEVELS,
            }),
            Ok(_) => Ok(()),
            Err(looped) => {
                let name = if changed.is_empty() {
                    groups
                        .into_iter()
                        .find(|group| group.id == looped)
                        .map(|group| group.name)
                        .unwrap_or_default()
                } else {
                    changed.to_owned()
                };
                Err(StoreError::GroupLoop { name })
            }
        }
    }

    /// How many groups the book has.
    fn group_count(&self) -> Result<u32, StoreError> {
        let count = self
            .conn
            .query_row("SELECT count(*) FROM report_groups", [], |row| row.get(0))?;
        Ok(count)
    }

    /// The groups' IDs, in the order of their list.
    fn group_ids(&self) -> Result<Vec<GroupId>, StoreError> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT id FROM report_groups ORDER BY position")?;
        let ids = statement
            .query_map([], |row| Ok(GroupId::from_bytes(row.get(0)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Numbers the groups 0, 1, 2, … in the given order; rows that already
    /// have their number are not touched, so they are not recorded.
    fn number_groups(&self, ids: &[GroupId]) -> Result<(), StoreError> {
        for (position, id) in (0_u32..).zip(ids) {
            self.conn.execute(
                "UPDATE report_groups SET position = ?2 WHERE id = ?1 AND position <> ?2",
                params![id.to_bytes(), position],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
