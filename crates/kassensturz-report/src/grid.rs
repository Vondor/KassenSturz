// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dashboard grid (M6b.1, D-287, D-296): its geometry, the smallest and
//! default size of each type, the frame checks, reading order, snapping and
//! the push-down rule every edit uses.
//!
//! A frame is in whole cells of a 12-column grid. No two frames overlap,
//! ever. When a frame lands on others they move down just far enough, and
//! push the ones below them in turn; nothing moves left, right or up by
//! itself, and gaps the user left stay. Everything here is integer
//! arithmetic: the screen hands in pixels as whole numbers and gets cells
//! back.

use crate::{ComponentType, Entry, Frame, Item, ReportError};

/// The number of columns of a dashboard.
pub const COLUMNS: u32 = 12;
/// The most rows one component may be high.
pub const MAX_ROWS: u32 = 24;
/// The lowest row a component may start at, the largest row number a
/// document keeps (D-281). An edit that would push a component below it is
/// refused as `no-room`.
pub const LAST_ROW: u32 = 1000;
/// A column narrower than this many grid units shows the dashboard as one
/// column in reading order (D-287).
pub const NARROW_COLUMN_UNITS: i64 = 3;

/// A width and height in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    /// Columns wide.
    pub cols: u32,
    /// Rows high.
    pub rows: u32,
}

impl ComponentType {
    /// The smallest frame this type stays readable in (D-287). A resize
    /// stops there.
    pub const fn smallest_size(self) -> Size {
        match self {
            Self::KpiCard => Size { cols: 2, rows: 2 },
            Self::ProgressBar | Self::Heading => Size { cols: 3, rows: 1 },
            Self::TableRows => Size { cols: 4, rows: 3 },
            Self::ChartLine
            | Self::ChartBar
            | Self::ChartStackedBar
            | Self::ChartArea
            | Self::ChartPie
            | Self::ChartDonut => Size { cols: 3, rows: 3 },
        }
    }

    /// The frame a new component of this type gets (D-296): half the width
    /// for a chart, a table or a progress bar, a quarter for a key figure
    /// card.
    pub const fn default_size(self) -> Size {
        match self {
            Self::KpiCard => Size { cols: 3, rows: 2 },
            Self::ProgressBar => Size { cols: 6, rows: 1 },
            Self::Heading => Size { cols: 12, rows: 1 },
            Self::TableRows
            | Self::ChartLine
            | Self::ChartBar
            | Self::ChartStackedBar
            | Self::ChartArea
            | Self::ChartPie
            | Self::ChartDonut => Size { cols: 6, rows: 4 },
        }
    }
}

impl Frame {
    /// The column just right of the frame.
    pub const fn right(self) -> u32 {
        self.col + self.cols
    }

    /// The row just below the frame.
    pub const fn bottom(self) -> u32 {
        self.row + self.rows
    }

    /// Whether the two frames share a cell.
    pub const fn overlaps(self, other: Self) -> bool {
        self.col < other.right()
            && other.col < self.right()
            && self.row < other.bottom()
            && other.row < self.bottom()
    }
}

/// The smallest frame of what a document holds in one place. A component of
/// a type this version doesn't know may be as small as one cell.
pub const fn smallest_size(item: &Item) -> Size {
    match item {
        Item::Component(component) => component.kind().smallest_size(),
        Item::Unknown(_) => Size { cols: 1, rows: 1 },
    }
}

fn invalid(reason: &'static str, index: usize) -> ReportError {
    ReportError::DocumentInvalid {
        reason,
        path: format!("components.{index}.frame"),
    }
}

/// Checks the frames of a dashboard's components: each has one, inside the
/// 12 columns, at most 24 rows high, starting at most at [`LAST_ROW`] and not
/// smaller than its type allows, and no two overlap. The path names the
/// component; for an overlap, the later of the two.
pub fn check(entries: &[Entry]) -> Result<(), ReportError> {
    for (index, entry) in entries.iter().enumerate() {
        let frame = entry.frame.ok_or_else(|| invalid("bad-frame", index))?;
        if frame.cols > COLUMNS
            || frame.right() > COLUMNS
            || frame.rows > MAX_ROWS
            || frame.row > LAST_ROW
        {
            return Err(invalid("frame-outside-grid", index));
        }
        let least = smallest_size(&entry.item);
        if frame.cols < least.cols || frame.rows < least.rows {
            return Err(invalid("frame-too-small", index));
        }
        if entries[..index]
            .iter()
            .any(|earlier| earlier.frame.is_some_and(|other| other.overlaps(frame)))
        {
            return Err(invalid("frames-overlap", index));
        }
    }
    Ok(())
}

/// The positions of the entries in reading order: by row, then by column,
/// top left first (D-287). Tab order, the screen reader and a narrow window
/// follow it. An entry without a frame comes last, in document order.
pub fn reading_order(entries: &[Entry]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&index| {
        entries[index]
            .frame
            .map_or((u32::MAX, u32::MAX), |frame| (frame.row, frame.col))
    });
    order
}

/// The nearest whole cell to an offset in pixels, for cells of `cell`
/// pixels; a half rounds to the next cell. The caller clamps the result to
/// the grid. A cell of no width gives 0.
pub fn snap(offset: i64, cell: i64) -> i64 {
    if cell <= 0 {
        return 0;
    }
    offset
        .saturating_mul(2)
        .saturating_add(cell)
        .div_euclid(cell.saturating_mul(2))
}

/// Whether a page of this width, with this gap between two columns, is too
/// narrow for the columns: a column would be narrower than
/// [`NARROW_COLUMN_UNITS`] grid units (D-287).
pub fn is_narrow(page_width: i64, gap: i64, grid_unit: i64) -> bool {
    let columns = i64::from(COLUMNS);
    let room = page_width.saturating_sub(gap.saturating_mul(columns - 1));
    room < grid_unit.saturating_mul(NARROW_COLUMN_UNITS * columns)
}

pub(crate) fn refused(reason: &'static str) -> ReportError {
    ReportError::GridEditRefused { reason }
}

/// Puts the frames at `first`, in this order, and then all others in
/// reading order, each where it is or, if it lands on one already put,
/// just below it. So the frames at `first` win, a frame moves only down and
/// only as far as it must, and one that lands on nothing stays.
pub(crate) fn push_down(frames: &mut [Frame], first: &[usize]) -> Result<(), ReportError> {
    let mut rest: Vec<usize> = (0..frames.len())
        .filter(|index| !first.contains(index))
        .collect();
    rest.sort_by_key(|&index| (frames[index].row, frames[index].col));
    let mut put: Vec<Frame> = Vec::with_capacity(frames.len());
    for index in first.iter().copied().chain(rest) {
        let mut frame = frames[index];
        // Every row above the lowest bottom it lands on still shares a cell
        // with that frame, so this is the nearest row that fits.
        while let Some(below) = put
            .iter()
            .filter(|other| other.overlaps(frame))
            .map(|other| other.bottom())
            .max()
        {
            frame.row = below;
        }
        if frame.row > LAST_ROW {
            return Err(refused("no-room"));
        }
        frames[index] = frame;
        put.push(frame);
    }
    Ok(())
}

/// The first place in reading order where a frame of this size overlaps
/// none of `frames`; below everything if there is none.
pub(crate) fn first_free(frames: &[Frame], size: Size) -> Result<Frame, ReportError> {
    let lowest = frames.iter().map(|frame| frame.bottom()).max().unwrap_or(0);
    for row in 0..=lowest.min(LAST_ROW) {
        for col in 0..=COLUMNS.saturating_sub(size.cols) {
            let frame = Frame {
                col,
                row,
                cols: size.cols,
                rows: size.rows,
            };
            if !frames.iter().any(|other| other.overlaps(frame)) {
                return Ok(frame);
            }
        }
    }
    Err(refused("no-room"))
}
