// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Custom charts from a Vega-Lite subset: the chart model (M6e.1a, D-502,
//! D-503).
//!
//! This module holds the types a chart description is read into, the table
//! of fields a description can name, and the parser [`read`] (M6e.1, D-505
//! to D-507), the one way in for a pasted description and an opened file.
//! Nothing here draws (the renderer is M6e.2). Every constructor
//! enforces the rules of `docs/REPORT_DESIGNER_M6E.md` sections 1 and 2, so a
//! value that breaks one cannot be built. There is no floating point: numbers
//! from a description are kept as digits and decimal places (D-506).

mod fields;
mod limits;
mod model;
mod parse;
mod unsupported;

#[cfg(test)]
mod fields_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod parse_tests;

pub use fields::{FieldValue, SPEC_DATASETS, SpecDataset, SpecField, spec_dataset};
pub use limits::{MAX_SPEC_BYTES, MAX_SPEC_DEPTH};
pub use model::{
    Aggregate, Channel, ChartSize, ChartSpec, Encoding, FieldFilter, FieldType, FilterTest, Layer,
    MAX_ENCODINGS, MAX_FILTERS, MAX_LAYERS, MAX_ONE_OF, MAX_PALETTE, MAX_SIZE, MAX_TEXT_CHARS,
    MIN_SIZE, Mark, MarkKind, ModelError, PaletteName, Sort, SpecNumber, SpecText, SpecValue,
    TimeUnit, VL_SUBSET,
};
pub use parse::{Read, SCHEMA_PREFIX, read};
pub use unsupported::{REASONS, Unsupported};
