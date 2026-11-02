// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The grid editor's session and its own undo (M6b.1, D-288, D-296).
//!
//! An [`Editor`] holds a dashboard while the user arranges it. Each add,
//! move, resize, duplicate, delete, property change and reset is one edit on
//! its command stack, at most [`MAX_UNDO`] of them; the oldest goes first.
//! The editor never writes to the book: when the user is done, the app
//! stores [`Editor::document`] as one history entry, and on Cancel it drops
//! the editor. Every edit is checked with [`grid::check`] before it counts,
//! so no edit can leave an overlap or a frame outside the grid; an edit that
//! is refused changes nothing.

use std::collections::{HashSet, VecDeque};

use kassensturz_core::ViewKind;

use crate::filter::{Dimension, check_list};
use crate::grid::{
    self, COLUMNS, LAST_ROW, MAX_ROWS, Size, first_free, is_box, push_down, refused,
};
use crate::{
    Component, ComponentType, Document, Entry, Filters, Frame, Item, MAX_COMPONENTS, PERIODS,
    ReportError, read_range_word,
};

/// The most edits the editor can take back (D-288).
pub const MAX_UNDO: usize = 200;

/// What one edit on the command stack did, for the menu's "Undo …" text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// A component was added from the library.
    Add,
    /// A component was moved, by dragging, by an arrow key, or earlier or
    /// later in a narrow window.
    Move,
    /// A component was made larger or smaller.
    Resize,
    /// A component was copied.
    Duplicate,
    /// A component was removed.
    Delete,
    /// A property, the dataset or the type of a component was changed.
    Change,
    /// The built-in layout replaced the components.
    Reset,
    /// A saved layout replaced the components.
    Load,
}

impl Edit {
    /// A fixed word for the screens, which keep the text (D-034).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Move => "move",
            Self::Resize => "resize",
            Self::Duplicate => "duplicate",
            Self::Delete => "delete",
            Self::Change => "change",
            Self::Reset => "reset",
            Self::Load => "load",
        }
    }
}

/// One edit on the stack, with the components as they were on its other
/// side.
#[derive(Clone, Debug)]
struct Step {
    edit: Edit,
    entries: Vec<Entry>,
}

/// A dashboard being arranged, with its own undo and redo.
#[derive(Clone, Debug)]
pub struct Editor {
    document: Document,
    start: Vec<Entry>,
    start_filters: Filters,
    undo: VecDeque<Step>,
    redo: Vec<Step>,
}

fn index_of(entries: &[Entry], id: &str) -> Result<usize, ReportError> {
    entries
        .iter()
        .position(|entry| entry.id == id)
        .ok_or_else(|| refused("unknown-component"))
}

fn frame_at(entries: &[Entry], index: usize) -> Result<Frame, ReportError> {
    entries[index]
        .frame
        .ok_or_else(|| ReportError::DocumentInvalid {
            reason: "bad-frame",
            path: format!("components.{index}.frame"),
        })
}

/// [`grid::check`], and that no two components have the same ID, which
/// [`Document::read`] checks too but a reset's entries might not have had.
fn check(entries: &[Entry]) -> Result<(), ReportError> {
    grid::check(entries)?;
    let mut ids = HashSet::new();
    match entries.iter().position(|entry| !ids.insert(&entry.id)) {
        Some(index) => Err(ReportError::DocumentInvalid {
            reason: "duplicate-id",
            path: format!("components.{index}.id"),
        }),
        None => Ok(()),
    }
}

/// Clamps a number of cells the screen asked for into `least..=most`.
fn cells(wanted: i64, least: u32, most: u32) -> u32 {
    u32::try_from(wanted.clamp(i64::from(least), i64::from(most.max(least)))).unwrap_or(least)
}

/// Gives the entry at `index` this frame and pushes the others down from it.
fn place(entries: &mut [Entry], index: usize, frame: Frame) -> Result<(), ReportError> {
    entries[index].frame = Some(frame);
    settle(entries, &[index])
}

/// [`push_down`] over the entries' frames, `first` first.
fn settle(entries: &mut [Entry], first: &[usize]) -> Result<(), ReportError> {
    let mut frames: Vec<Frame> = (0..entries.len())
        .map(|index| frame_at(entries, index))
        .collect::<Result<_, _>>()?;
    let boxes: Vec<bool> = entries.iter().map(|entry| is_box(&entry.item)).collect();
    push_down(&mut frames, &boxes, first)?;
    for (entry, frame) in entries.iter_mut().zip(frames) {
        entry.frame = Some(frame);
    }
    Ok(())
}

/// The first ID of the form `c1`, `c2`, … that no entry has.
fn new_id(entries: &[Entry]) -> String {
    (1..)
        .map(|n| format!("c{n}"))
        .find(|id| entries.iter().all(|entry| &entry.id != id))
        .unwrap_or_default()
}

impl Editor {
    /// Starts arranging a dashboard. A view, a dashboard with more than
    /// [`MAX_COMPONENTS`] components or one whose frames fail
    /// [`grid::check`] is refused, never repaired.
    pub fn new(document: Document) -> Result<Self, ReportError> {
        if document.kind != ViewKind::Dashboard {
            return Err(refused("not-a-dashboard"));
        }
        if document.entries.len() > MAX_COMPONENTS {
            return Err(refused("too-many-components"));
        }
        check(&document.entries)?;
        Ok(Self {
            start: document.entries.clone(),
            start_filters: document.filters(),
            document,
            undo: VecDeque::new(),
            redo: Vec::new(),
        })
    }

    /// The dashboard as it is now.
    pub const fn document(&self) -> &Document {
        &self.document
    }

    /// The dashboard as it is now, to store when the user is done.
    pub fn into_document(self) -> Document {
        self.document
    }

    /// The IDs of the components in reading order.
    pub fn reading_order(&self) -> Vec<&str> {
        grid::reading_order(&self.document.entries)
            .into_iter()
            .map(|index| self.document.entries[index].id.as_str())
            .collect()
    }

    /// Whether the dashboard differs from what the session started with, so
    /// Cancel must ask before throwing the changes away.
    pub fn has_changes(&self) -> bool {
        self.document.entries != self.start || self.document.filters() != self.start_filters
    }

    /// What the dashboard's own filter bar holds (D-351): the filters that
    /// are saved with the layout.
    pub fn filters(&self) -> Filters {
        self.document.filters()
    }

    /// Sets the filters the layout carries (M6c.7, D-351). They are not an
    /// edit on the stack: they are kept with the layout when the session is
    /// stored, as part of its one history entry, and dropped with it on
    /// Cancel. Refused: a list of more than [`crate::MAX_FILTER_IDS`] IDs or
    /// with one twice (`report.filter-invalid`); a period that is neither a
    /// fixed one nor a typed range (`report.document-invalid`). An ID the book no longer knows
    /// is kept, so the user can take it out. Gives `false` when nothing
    /// changed.
    pub fn set_filters(&mut self, filters: &Filters) -> Result<bool, ReportError> {
        if filters
            .period
            .as_deref()
            .is_some_and(|period| !PERIODS.contains(&period) && read_range_word(period).is_none())
        {
            return Err(ReportError::DocumentInvalid {
                reason: "bad-choice",
                path: "filters.period".into(),
            });
        }
        check_list(Dimension::Accounts, &filters.accounts)?;
        check_list(Dimension::Categories, &filters.categories)?;
        if self.document.filters() == *filters {
            return Ok(false);
        }
        self.document.period.clone_from(&filters.period);
        self.document.accounts.clone_from(&filters.accounts);
        self.document.categories.clone_from(&filters.categories);
        Ok(true)
    }

    /// The edit that Undo would take back, if any.
    pub fn next_undo(&self) -> Option<Edit> {
        self.undo.back().map(|step| step.edit)
    }

    /// The edit that Redo would make again, if any.
    pub fn next_redo(&self) -> Option<Edit> {
        self.redo.last().map(|step| step.edit)
    }

    /// Takes back the last edit; `None` when there is none.
    pub fn undo(&mut self) -> Option<Edit> {
        let step = self.undo.pop_back()?;
        let edit = step.edit;
        let now = std::mem::replace(&mut self.document.entries, step.entries);
        self.redo.push(Step { edit, entries: now });
        Some(edit)
    }

    /// Makes the last edit taken back again; `None` when there is none.
    pub fn redo(&mut self) -> Option<Edit> {
        let step = self.redo.pop()?;
        let edit = step.edit;
        let now = std::mem::replace(&mut self.document.entries, step.entries);
        self.undo.push_back(Step { edit, entries: now });
        Some(edit)
    }

    /// Makes one edit on a copy of the components, checks the result and
    /// keeps it as one step on the stack. Gives `false`, and keeps no step,
    /// when the edit changes nothing (a move to where the component is).
    fn apply(
        &mut self,
        edit: Edit,
        change: impl FnOnce(&mut Vec<Entry>) -> Result<(), ReportError>,
    ) -> Result<bool, ReportError> {
        let mut entries = self.document.entries.clone();
        change(&mut entries)?;
        check(&entries)?;
        if entries == self.document.entries {
            return Ok(false);
        }
        let before = std::mem::replace(&mut self.document.entries, entries);
        self.undo.push_back(Step {
            edit,
            entries: before,
        });
        if self.undo.len() > MAX_UNDO {
            self.undo.pop_front();
        }
        self.redo.clear();
        Ok(true)
    }

    /// Adds a component with its type's default size at the first free
    /// place in reading order, or below everything. Gives its new ID.
    pub fn add(&mut self, component: Component) -> Result<String, ReportError> {
        if self.document.entries.len() >= MAX_COMPONENTS {
            return Err(refused("too-many-components"));
        }
        let id = new_id(&self.document.entries);
        let new = id.clone();
        self.apply(Edit::Add, |entries| {
            let frames: Vec<(Frame, bool)> = entries
                .iter()
                .filter_map(|entry| entry.frame.map(|frame| (frame, is_box(&entry.item))))
                .collect();
            // A period box also keeps clear of the widgets: lying under them
            // it would be invisible and would silently give them its period
            // (M6-B12), so it gets one layer with everything else.
            let frames: Vec<(Frame, bool)> = if component.kind() == ComponentType::PeriodBox {
                frames
                    .into_iter()
                    .map(|(frame, _)| (frame, false))
                    .collect()
            } else {
                frames
            };
            // A widget's own layer is `false` (a box counts only against boxes
            // for it), so the box, whose frames were all set to `false`, uses it too.
            let frame = first_free(&frames, false, component.kind().default_size())?;
            entries.push(Entry {
                id: new,
                item: Item::Component(component),
                frame: Some(frame),
            });
            Ok(())
        })?;
        Ok(id)
    }

    /// Moves a component so its top left corner is at this cell, clamped to
    /// the grid, and pushes down what it lands on. The screen snaps a drag
    /// to a cell with [`grid::snap`].
    pub fn move_to(&mut self, id: &str, col: i64, row: i64) -> Result<bool, ReportError> {
        self.apply(Edit::Move, |entries| {
            let index = index_of(entries, id)?;
            let frame = frame_at(entries, index)?;
            let moved = Frame {
                col: cells(col, 0, COLUMNS - frame.cols),
                row: cells(row, 0, LAST_ROW),
                ..frame
            };
            let mut first = vec![index];
            // A period box takes the widgets that lie inside it along (D-333).
            if is_box(&entries[index].item) {
                let (dcol, drow) = (
                    i64::from(moved.col) - i64::from(frame.col),
                    i64::from(moved.row) - i64::from(frame.row),
                );
                for other in 0..entries.len() {
                    let inner = frame_at(entries, other)?;
                    if other == index || is_box(&entries[other].item) || !frame.contains(inner) {
                        continue;
                    }
                    let row = i64::from(inner.row) + drow;
                    if row > i64::from(LAST_ROW) {
                        return Err(refused("no-room"));
                    }
                    entries[other].frame = Some(Frame {
                        col: u32::try_from(i64::from(inner.col) + dcol).unwrap_or(0),
                        row: u32::try_from(row).unwrap_or(0),
                        ..inner
                    });
                    first.push(other);
                }
            }
            entries[index].frame = Some(moved);
            settle(entries, &first)
        })
    }

    /// Moves a component by whole cells, as the arrow keys do.
    pub fn nudge(&mut self, id: &str, cols: i64, rows: i64) -> Result<bool, ReportError> {
        let frame = frame_at(
            &self.document.entries,
            index_of(&self.document.entries, id)?,
        )?;
        self.move_to(
            id,
            i64::from(frame.col).saturating_add(cols),
            i64::from(frame.row).saturating_add(rows),
        )
    }

    /// Gives a component this size, keeping its top left corner, clamped to
    /// the grid and to its type's smallest size, and pushes down what it
    /// lands on.
    pub fn resize_to(&mut self, id: &str, cols: i64, rows: i64) -> Result<bool, ReportError> {
        self.apply(Edit::Resize, |entries| {
            let index = index_of(entries, id)?;
            let frame = frame_at(entries, index)?;
            let least = grid::smallest_size(&entries[index].item);
            let resized = Frame {
                cols: cells(cols, least.cols, COLUMNS - frame.col),
                rows: cells(rows, least.rows, MAX_ROWS),
                ..frame
            };
            place(entries, index, resized)
        })
    }

    /// Makes a component wider or narrower and taller or lower by whole
    /// cells, as Shift and the arrow keys do.
    pub fn grow(&mut self, id: &str, cols: i64, rows: i64) -> Result<bool, ReportError> {
        let frame = frame_at(
            &self.document.entries,
            index_of(&self.document.entries, id)?,
        )?;
        self.resize_to(
            id,
            i64::from(frame.cols).saturating_add(cols),
            i64::from(frame.rows).saturating_add(rows),
        )
    }

    /// Copies a component under a new ID: right of the original if it fits
    /// there in the same rows, else directly below it, pushing down what is
    /// there. Gives the new ID.
    pub fn duplicate(&mut self, id: &str) -> Result<String, ReportError> {
        if self.document.entries.len() >= MAX_COMPONENTS {
            return Err(refused("too-many-components"));
        }
        let copy = new_id(&self.document.entries);
        let new = copy.clone();
        self.apply(Edit::Duplicate, |entries| {
            let index = index_of(entries, id)?;
            let frame = frame_at(entries, index)?;
            let right = Frame {
                col: frame.right(),
                ..frame
            };
            let layer = is_box(&entries[index].item);
            let fits_right = right.right() <= COLUMNS
                && !entries.iter().any(|entry| {
                    is_box(&entry.item) == layer
                        && entry.frame.is_some_and(|other| other.overlaps(right))
                });
            let item = entries[index].item.clone();
            entries.push(Entry {
                id: new,
                item,
                frame: Some(right),
            });
            if fits_right {
                Ok(())
            } else {
                let below = Frame {
                    row: frame.bottom(),
                    ..frame
                };
                let last = entries.len() - 1;
                place(entries, last, below)
            }
        })?;
        Ok(copy)
    }

    /// Removes a component; the others stay where they are.
    pub fn delete(&mut self, id: &str) -> Result<bool, ReportError> {
        self.apply(Edit::Delete, |entries| {
            let index = index_of(entries, id)?;
            entries.remove(index);
            Ok(())
        })
    }

    /// Changes a component through `change` ([`Component::set`],
    /// [`Component::use_dataset`] or [`Component::change_kind`]). When its
    /// new type needs a larger frame, the frame grows to the smallest size,
    /// moves left only as far as it must to stay in the grid, and pushes
    /// down what it lands on. A component of a type this version doesn't
    /// know can't be changed.
    pub fn change(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut Component) -> Result<(), ReportError>,
    ) -> Result<bool, ReportError> {
        self.apply(Edit::Change, |entries| {
            let index = index_of(entries, id)?;
            let Item::Component(component) = &mut entries[index].item else {
                return Err(refused("not-editable"));
            };
            change(component)?;
            let least: Size = component.kind().smallest_size();
            let frame = frame_at(entries, index)?;
            let cols = frame.cols.max(least.cols);
            let grown = Frame {
                col: frame.col.min(COLUMNS - cols),
                cols,
                rows: frame.rows.max(least.rows),
                ..frame
            };
            place(entries, index, grown)
        })
    }

    /// Moves a component one place earlier in reading order, as a narrow
    /// window offers it. It takes the first place after the components that
    /// come before its neighbour where it fits among them, at most its own
    /// place, so it lands exactly one place earlier; the neighbour takes its
    /// old row (moving left only as far as it must to stay in the grid) and
    /// goes after it; then what they land on is pushed down. The components
    /// before them stay. Gives `false` for the first component.
    pub fn move_earlier(&mut self, id: &str) -> Result<bool, ReportError> {
        self.apply(Edit::Move, |entries| {
            let index = index_of(entries, id)?;
            let layer = is_box(&entries[index].item);
            let mut order = grid::reading_order(entries);
            // Boxes and widgets are ordered among their own kind (D-333).
            order.retain(|&i| is_box(&entries[i].item) == layer);
            let at = order.iter().position(|&i| i == index).unwrap_or(0);
            let Some(&neighbour) = at.checked_sub(1).and_then(|at| order.get(at)) else {
                return Ok(());
            };
            let earlier = &order[..at - 1];
            let fixed: Vec<Frame> = earlier
                .iter()
                .map(|&i| frame_at(entries, i))
                .collect::<Result<_, _>>()?;
            let (this, that) = (frame_at(entries, index)?, frame_at(entries, neighbour)?);
            // Its own place always fits, so the search ends there at last.
            let after = fixed.last().map(|frame| (frame.row, frame.col));
            let moved = (0..=this.row)
                .flat_map(|row| (0..=COLUMNS - this.cols).map(move |col| (row, col)))
                .filter(|&place| after.is_none_or(|after| place > after))
                .take_while(|&place| place <= (this.row, this.col))
                .map(|(row, col)| Frame { col, row, ..this })
                .find(|frame| !fixed.iter().any(|other| other.overlaps(*frame)))
                .unwrap_or(this);
            let mut other = Frame {
                col: this.col.min(COLUMNS - that.cols),
                row: this.row,
                ..that
            };
            if (other.row, other.col) <= (moved.row, moved.col) {
                other.row = moved.row + 1;
            }
            entries[index].frame = Some(moved);
            entries[neighbour].frame = Some(other);
            let mut first = earlier.to_vec();
            first.extend([index, neighbour]);
            settle(entries, &first)
        })
    }

    /// Moves a component one place later in reading order: the component
    /// after it moves earlier. Gives `false` for the last component.
    pub fn move_later(&mut self, id: &str) -> Result<bool, ReportError> {
        let entries = &self.document.entries;
        let index = index_of(entries, id)?;
        let layer = is_box(&entries[index].item);
        let mut order = grid::reading_order(entries);
        order.retain(|&i| is_box(&entries[i].item) == layer);
        let at = order.iter().position(|&i| i == index).unwrap_or(0);
        match order.get(at + 1) {
            Some(&after) => {
                let after = entries[after].id.clone();
                self.move_earlier(&after)
            }
            None => Ok(false),
        }
    }

    /// Replaces the components with the built-in layout's, as one edit that
    /// Undo takes back. The entries are checked like a dashboard's.
    pub fn reset(&mut self, built_in: &[Entry]) -> Result<bool, ReportError> {
        self.replace_all(Edit::Reset, built_in)
    }

    /// Replaces the components with a saved layout's (D-330), as one edit
    /// that Undo takes back. The entries are checked like a dashboard's.
    pub fn load(&mut self, saved: &[Entry]) -> Result<bool, ReportError> {
        self.replace_all(Edit::Load, saved)
    }

    fn replace_all(&mut self, edit: Edit, replacement: &[Entry]) -> Result<bool, ReportError> {
        if replacement.len() > MAX_COMPONENTS {
            return Err(refused("too-many-components"));
        }
        self.apply(edit, |entries| {
            replacement.clone_into(entries);
            Ok(())
        })
    }
}
