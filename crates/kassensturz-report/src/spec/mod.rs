// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Custom charts from a Vega-Lite subset: the chart model (M6e.1a, D-502,
//! D-503).
//!
//! This module holds only the types a chart description is read into and the
//! table of fields a description can name. Nothing here reads JSON (the
//! parser is M6e.1) or draws (the renderer is M6e.2). Every constructor
//! enforces the rules of `docs/REPORT_DESIGNER_M6E.md` sections 1 and 2, so a
//! value that breaks one cannot be built. There is no floating point: numbers
//! from a description are kept as digits and decimal places (D-506).

mod fields;
mod model;

#[cfg(test)]
mod fields_tests;
#[cfg(test)]
mod model_tests;

pub use fields::{FieldValue, SPEC_DATASETS, SpecDataset, SpecField, spec_dataset};
pub use model::{
    Aggregate, Channel, ChartSize, ChartSpec, Encoding, FieldFilter, FieldType, FilterTest, Layer,
    MAX_ENCODINGS, MAX_FILTERS, MAX_LAYERS, MAX_ONE_OF, MAX_PALETTE, MAX_SIZE, MAX_TEXT_CHARS,
    MIN_SIZE, Mark, MarkKind, ModelError, PaletteName, Sort, SpecNumber, SpecText, SpecValue,
    TimeUnit, VL_SUBSET,
};
