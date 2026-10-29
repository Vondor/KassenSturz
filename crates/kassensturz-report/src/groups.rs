// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! My groups and the hide list in the datasets (M6b.4, D-291, D-304).
//!
//! A group or the hide list never changes a figure of the book: it only
//! changes how a chart or a table puts the figures together. A category in a
//! group counts with everything inside it, and once however often it is
//! reached. A group always counts all its members, hidden ones included
//! (Q-225); the hide list only takes categories out where they would be
//! shown on their own, and the dataset says how much it left out.

use std::collections::{HashMap, HashSet};

use kassensturz_core::{AccountId, GroupId, GroupMember};

use crate::{ReportError, Source};

/// Whether a chart may count one category in two of its rows or series.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overlap {
    /// The chart shows parts of a whole (a pie, a donut, stacked bars, the
    /// shares of a table): two chosen groups or series that share a category
    /// are refused with `report.groups-overlap`, since the whole would count
    /// it twice. The default, so a caller that forgets is safe.
    #[default]
    Refused,
    /// The chart shows separate series (lines, areas, side-by-side bars),
    /// where a category may be in more than one.
    Allowed,
}

/// How a dataset by category uses My groups and the hide list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// The groups shown as one row each, in this order (`params.groups`).
    /// Every category in none of them is listed on its own, as without
    /// groups. A group the book doesn't know is skipped
    /// ([`Source::unknown`] says which); one given twice counts once.
    pub groups: Vec<GroupId>,
    /// Show the hidden categories anyway (`props.show_hidden`).
    pub show_hidden: bool,
    /// Whether two chosen groups may share a category.
    pub overlap: Overlap,
}

impl Shown {
    /// Every category on its own and nothing hidden, as the charts showed
    /// them before My groups (M6a).
    pub fn everything() -> Self {
        Self {
            show_hidden: true,
            ..Self::default()
        }
    }
}

/// A chosen group or series with the categories it counts.
pub(crate) struct Resolved {
    pub(crate) of: GroupMember,
    pub(crate) path: Vec<String>,
    pub(crate) categories: HashSet<AccountId>,
}

impl Source {
    /// The categories `of` counts: a category with everything inside it, a
    /// group with all its members and the members of the groups inside it,
    /// each category once, in the order of the category tree. Empty when the
    /// book has no such category or group.
    pub fn counted(&self, of: GroupMember) -> Vec<AccountId> {
        let Some(counted) = self.counted_set(of) else {
            return Vec::new();
        };
        self.tree()
            .iter()
            .map(|entry| entry.category.id)
            .filter(|id| counted.contains(id))
            .collect()
    }

    /// The category or group whose ID is written as `text` in a document
    /// (`params.groups`, `params.series`), `None` when the book doesn't know
    /// it. A document is data: nothing is parsed, the text is only compared
    /// with the IDs the book has, ignoring capitals.
    pub fn member_by_id(&self, text: &str) -> Option<GroupMember> {
        let same = |id: &dyn std::fmt::Display| id.to_string().eq_ignore_ascii_case(text);
        self.groups()
            .iter()
            .find(|group| same(&group.id))
            .map(|group| GroupMember::Group(group.id))
            .or_else(|| {
                self.tree()
                    .iter()
                    .find(|entry| same(&entry.category.id))
                    .map(|entry| GroupMember::Category(entry.category.id))
            })
    }

    /// The chosen categories and groups the book doesn't know (any more), in
    /// the order given, so a chart can say "A group in this chart no longer
    /// exists" instead of silently showing less.
    pub fn unknown(&self, chosen: &[GroupMember]) -> Vec<GroupMember> {
        chosen
            .iter()
            .copied()
            .filter(|&of| self.path_of(of).is_none())
            .collect()
    }

    /// The category's path from the top-level category down, or the group's
    /// name as a path of one, or `None` if the book doesn't know it.
    pub(crate) fn path_of(&self, of: GroupMember) -> Option<Vec<String>> {
        match of {
            GroupMember::Category(id) => self
                .tree()
                .iter()
                .find(|entry| entry.category.id == id)
                .map(|entry| entry.path.clone()),
            GroupMember::Group(id) => self.group(id).map(|group| vec![group.name.clone()]),
        }
    }

    /// The set behind [`Source::counted`], `None` for an unknown one.
    fn counted_set(&self, of: GroupMember) -> Option<HashSet<AccountId>> {
        self.path_of(of)?;
        let mut roots = HashSet::new();
        let mut seen = HashSet::new();
        let mut waiting = vec![of];
        while let Some(member) = waiting.pop() {
            match member {
                GroupMember::Category(id) => {
                    roots.insert(id);
                }
                // The book refuses loops (D-301); `seen` keeps a wrong
                // `Source` from going round for ever all the same.
                GroupMember::Group(id) => {
                    if seen.insert(id)
                        && let Some(group) = self.group(id)
                    {
                        waiting.extend(group.members.iter().copied());
                    }
                }
            }
        }
        Some(self.inside(&roots).into_keys().collect())
    }

    /// The chosen categories and groups the book knows, each once, with what
    /// they count, refusing two that share a category if `overlap` says so.
    pub(crate) fn resolve(
        &self,
        chosen: &[GroupMember],
        overlap: Overlap,
    ) -> Result<Vec<Resolved>, ReportError> {
        let mut resolved: Vec<Resolved> = Vec::new();
        for &of in chosen {
            if resolved.iter().any(|known| known.of == of) {
                continue;
            }
            let (Some(path), Some(categories)) = (self.path_of(of), self.counted_set(of)) else {
                continue;
            };
            resolved.push(Resolved {
                of,
                path,
                categories,
            });
        }
        if overlap == Overlap::Refused {
            for (at, first) in resolved.iter().enumerate() {
                for second in &resolved[at + 1..] {
                    // The first shared category in the tree's order, so the
                    // message names the same one every time.
                    if let Some(shared) = self.tree().iter().find(|entry| {
                        first.categories.contains(&entry.category.id)
                            && second.categories.contains(&entry.category.id)
                    }) {
                        return Err(ReportError::GroupsOverlap {
                            category: shared.path.join(" › "),
                            first: first.path.join(" › "),
                            second: second.path.join(" › "),
                        });
                    }
                }
            }
        }
        Ok(resolved)
    }

    /// Each hidden category and everything inside it, with the hidden
    /// category it is hidden by (the topmost, if a hidden one is inside
    /// another).
    pub(crate) fn hidden_by(&self) -> HashMap<AccountId, AccountId> {
        let roots: HashSet<AccountId> = self.hidden().iter().copied().collect();
        self.inside(&roots)
    }
}
