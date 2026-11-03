// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The renderer of custom charts (M6e.2, D-503): a [`ChartSpec`] and the flat
//! table of its dataset become a [`ChartModel`] the chart components draw.
//!
//! The renderer lives here and not in `kassensturz-report` because the chart
//! model is this crate's and this crate depends on the report crate, never
//! the other way round. It reads the table and nothing else, so a description
//! can't reach anything the dataset doesn't hold.
//!
//! Everything is exact: sums, minimums and maximums are integers in minor
//! units, a mean is rounded once with the book's rounding mode (D-027), a
//! filter compares an amount with a number by scaling integers (D-506). The
//! only floating point of a chart is the drawing values of
//! [`ChartProvider::prepare`](crate::ChartProvider::prepare).
//!
//! **Layers.** The first line, bar or area layer is the chart itself
//! ([`ChartModel::kind`] and [`ChartModel::series`]); every other layer is an
//! [`Overlay`] on the same axes. A description with only points, texts or
//! rules gets an empty bar chart under its overlays. All layers share one
//! category axis (the model checked that) and one kind of value: amounts,
//! counts or percentages.

use std::cmp::Ordering;
use std::collections::HashMap;

use kassensturz_core::{Date, ErrorReport, Money, Report, RoundingMode};
use kassensturz_report::spec::{
    Aggregate, Cell, Channel, ChartSpec, Encoding, FieldFilter, FieldType, FieldValue, FilterTest,
    FlatTable, Layer, MarkKind, Rgb, Sort, SpecColor, SpecField, SpecValue, TimeUnit,
};
use kassensturz_report::{Percent, ReportError, TextFormat, TextTable};

use crate::datasets::{ShownRow, extent, major_unit, round_slices};
use crate::ticks::{ticks, ticks_with};
use crate::{
    ANSWER_NO, ANSWER_YES, ChartError, ChartKind, ChartModel, MAX_DOT_SIZE, Overlay, OverlayItem,
    OverlayMark, OverlaySeries, PALETTE_SIZE, Point, Series, Slices, Tick,
};

/// Most rows a chart may use after its filters (D-281), and most points it
/// may draw.
pub const MAX_ROWS: usize = 10_000;
/// Most series or slices of a chart (D-281).
pub const MAX_SERIES: usize = 50;
/// What an empty point says in the table and to a screen reader: a series
/// that has nothing at that place.
const NOTHING: &str = "–";

/// A custom chart: the model the components draw and what the description
/// says about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomChart {
    /// The chart. `total` is empty: a description has no fixed meaning for
    /// a total.
    pub chart: ChartModel,
    /// The description's `title`, the user's own text.
    pub title: Option<String>,
    /// The description's `description`, the user's own text.
    pub description: Option<String>,
    /// The width the description asks for, 100 to 2000.
    pub width: Option<u16>,
    /// The height the description asks for, 100 to 2000.
    pub height: Option<u16>,
}

/// Why a chart couldn't be made. The words are those of the error codes of
/// `docs/REPORT_DESIGNER_M6E.md` section 5; the caller turns them into
/// `report.spec-too-big` and `report.spec-invalid` (D-034).
#[derive(Debug, PartialEq, Eq)]
pub enum CustomError {
    /// `report.spec-too-big` with this limit: `rows` or `series`.
    TooBig(&'static str),
    /// `report.spec-invalid` with this reason.
    Invalid(&'static str),
    /// A total or a tick doesn't fit, or a value is too large to draw.
    Chart(ChartError),
}

impl From<ChartError> for CustomError {
    fn from(error: ChartError) -> Self {
        Self::Chart(error)
    }
}

impl Report for CustomError {
    /// The error a screen shows: the two report codes of a description that
    /// is too big or can't be drawn (the same ones the parser uses, with no
    /// path because the renderer reads a model, not text), or the chart
    /// code of an amount that doesn't fit.
    fn report(&self) -> ErrorReport {
        match self {
            Self::TooBig(limit) => ReportError::SpecTooBig { limit }.report(),
            Self::Invalid(reason) => ReportError::SpecInvalid {
                reason,
                path: String::new(),
            }
            .report(),
            Self::Chart(error) => error.report(),
        }
    }
}

/// Draws `spec` from `table`: `Ok(None)` when no row is left after the
/// filters, so the screen says there is nothing to show.
///
/// # Errors
///
/// [`CustomError::TooBig`] for more than [`MAX_ROWS`] rows or
/// [`MAX_SERIES`] series, [`CustomError::Invalid`] for a description that
/// can't be drawn from this dataset, [`CustomError::Chart`] for amounts that
/// overflow.
pub fn custom_chart(
    spec: &ChartSpec,
    table: &FlatTable,
    format: &impl TextFormat,
) -> Result<Option<CustomChart>, CustomError> {
    if table.dataset().id() != spec.dataset().id() {
        return Err(CustomError::Invalid("unknown-dataset"));
    }
    let renderer = Renderer { table, format };
    renderer.render(spec)
}

/// What the amounts of a chart are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quantity {
    /// Money of the table's currency, in minor units.
    Amount,
    /// A number of rows, in hundredths.
    Count,
    /// A percentage, in tenths.
    Percent,
}

/// Where a mark goes on the category axis, or what makes a series.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Key {
    Text(String),
    Day(Date),
    Bool(bool),
}

/// What a sort by another field compares.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SortKey {
    Number(i128),
    Day(Date),
    Text(String),
}

/// The encodings of one layer by channel.
#[derive(Default)]
struct Channels<'a> {
    x: Option<&'a Encoding>,
    y: Option<&'a Encoding>,
    color: Option<&'a Encoding>,
    size: Option<&'a Encoding>,
    theta: Option<&'a Encoding>,
    text: Option<&'a Encoding>,
    tooltips: Vec<&'a Encoding>,
}

impl<'a> Channels<'a> {
    /// The layer's encodings; a text layer without an `x` or `y` takes
    /// them from the layer below it (design section 1).
    fn of(layer: &'a Layer, below: Option<&Channels<'a>>) -> Self {
        let mut channels = Self::default();
        for encoding in layer.encodings() {
            match encoding.channel() {
                Channel::X => channels.x = Some(encoding),
                Channel::Y => channels.y = Some(encoding),
                Channel::Color => channels.color = Some(encoding),
                Channel::Size => channels.size = Some(encoding),
                Channel::Theta => channels.theta = Some(encoding),
                Channel::Text => channels.text = Some(encoding),
                Channel::Tooltip => channels.tooltips.push(encoding),
            }
        }
        if layer.mark().kind() == MarkKind::Text
            && let Some(below) = below
        {
            channels.x = channels.x.or(below.x);
            channels.y = channels.y.or(below.y);
        }
        channels
    }

    /// The name of an uncolored series: the user's title, else the name of
    /// the field they wrote.
    fn default_name(&self) -> String {
        [self.y, self.theta, self.text, self.x]
            .into_iter()
            .flatten()
            .map(|encoding| {
                encoding.title().map_or_else(
                    || encoding.field().name().to_owned(),
                    |t| t.as_str().to_owned(),
                )
            })
            .next()
            .unwrap_or_default()
    }
}

/// The rows of a layer by (series, place).
type Groups<'r> = HashMap<(usize, usize), Vec<&'r [Cell]>>;

/// A series of rules across the chart: its name, color, and the amount and
/// words of each level.
type Levels = (String, u8, Vec<(i64, String)>);

/// One series of an aligned layer: a value (and the words around it) for
/// every place of the axis, or none.
struct Aligned {
    name: String,
    color: u8,
    values: Vec<Option<i64>>,
    labels: Vec<String>,
    texts: Vec<String>,
    raw_sizes: Vec<Option<i64>>,
    sizes: Vec<u8>,
}

struct Renderer<'a, F> {
    table: &'a FlatTable,
    format: &'a F,
}

impl<F: TextFormat> Renderer<'_, F> {
    fn index_of(&self, field: &SpecField) -> usize {
        self.table
            .dataset()
            .fields()
            .iter()
            .position(|own| own.name() == field.name())
            .unwrap_or(0)
    }

    fn amount_label(&self, minor_units: i64) -> String {
        self.format
            .amount(Money::from_minor_units(minor_units, self.table.currency()))
    }

    fn label(&self, quantity: Quantity, value: i64) -> String {
        match quantity {
            Quantity::Amount => self.amount_label(value),
            Quantity::Count => self.format.number(value),
            Quantity::Percent => self.format.percent(Percent::from_tenths(value)),
        }
    }

    // ---- filters ----

    /// How a cell compares with a value of a filter; `None` when the cell
    /// holds nothing (a percentage with nothing to compare with).
    fn compare(&self, cell: &Cell, value: &SpecValue) -> Option<Ordering> {
        match (cell, value) {
            (Cell::Text(a), SpecValue::Text(b)) => Some(a.as_str().cmp(b.as_str())),
            (Cell::Day(a), SpecValue::Day(b)) => Some(a.cmp(b)),
            (Cell::YesNo(a), SpecValue::YesNo(b)) => Some(a.cmp(b)),
            (Cell::Amount(money), SpecValue::Number(number)) => Some(scaled(
                i128::from(money.minor_units()),
                self.table.decimals(),
                i128::from(number.digits()),
                number.places(),
            )),
            (Cell::Percent(percent), SpecValue::Number(number)) => percent.map(|percent| {
                scaled(
                    i128::from(percent.tenths()),
                    1,
                    i128::from(number.digits()),
                    number.places(),
                )
            }),
            _ => None,
        }
    }

    fn passes(&self, filter: &FieldFilter, row: &[Cell]) -> bool {
        let cell = &row[self.index_of(filter.field())];
        let is = |value: &SpecValue, wanted: &[Ordering]| {
            self.compare(cell, value)
                .is_some_and(|found| wanted.contains(&found))
        };
        match filter.test() {
            FilterTest::Equal(value) => is(value, &[Ordering::Equal]),
            FilterTest::OneOf(values) => values.iter().any(|value| is(value, &[Ordering::Equal])),
            FilterTest::Range { from, to } => {
                from.as_ref()
                    .is_none_or(|from| is(from, &[Ordering::Greater, Ordering::Equal]))
                    && to
                        .as_ref()
                        .is_none_or(|to| is(to, &[Ordering::Less, Ordering::Equal]))
            }
        }
    }

    /// The rows a layer uses: the table's rows that pass the description's
    /// filters and the layer's own (design section 2).
    fn rows_of<'t>(
        &'t self,
        spec: &ChartSpec,
        layer: &Layer,
    ) -> Result<Vec<&'t [Cell]>, CustomError> {
        let rows: Vec<&[Cell]> = self
            .table
            .rows()
            .iter()
            .map(Vec::as_slice)
            .filter(|row| {
                spec.filters()
                    .iter()
                    .chain(layer.filters())
                    .all(|filter| self.passes(filter, row))
            })
            .collect();
        if rows.len() > MAX_ROWS {
            return Err(CustomError::TooBig("rows"));
        }
        Ok(rows)
    }

    // ---- places and series ----

    fn unit_of(encoding: &Encoding) -> Option<TimeUnit> {
        encoding.time_unit().or_else(|| {
            (encoding.field().value() == FieldValue::Day && encoding.field().name() == "month")
                .then_some(TimeUnit::YearMonth)
        })
    }

    /// The key of a cell as a place or a series, cut to the time unit.
    fn key(cell: &Cell, unit: Option<TimeUnit>) -> Result<Key, CustomError> {
        Ok(match cell {
            Cell::Text(text) => Key::Text(text.clone()),
            Cell::Day(day) => Key::Day(match unit {
                Some(TimeUnit::Year) => day.first_of_year(),
                Some(TimeUnit::YearMonth) => day.first_of_month(),
                Some(TimeUnit::YearMonthDate) | None => *day,
            }),
            Cell::YesNo(value) => Key::Bool(*value),
            // An amount is not a place; the model keeps these out of `x` and
            // `color`, and a count on `x` is not a place either.
            Cell::Amount(_) | Cell::Percent(_) => {
                return Err(CustomError::Invalid("type-mismatch"));
            }
        })
    }

    fn key_text(&self, key: &Key, unit: Option<TimeUnit>) -> String {
        match key {
            // Money with no category yet: the screen writes its name (D-034).
            Key::Text(text) if text.is_empty() => self.format.unsorted(),
            Key::Text(text) => text.clone(),
            Key::Day(day) => match unit {
                Some(TimeUnit::Year) => day.year().to_string(),
                Some(TimeUnit::YearMonth) => self.format.month(*day),
                Some(TimeUnit::YearMonthDate) | None => self.format.day(*day),
            },
            // The screen writes the word in the user's language (D-525).
            Key::Bool(value) => (if *value { ANSWER_YES } else { ANSWER_NO }).to_string(),
        }
    }

    /// What a sort by another field compares for the rows at one place.
    fn sort_key(&self, rows: &[&[Cell]], field: &SpecField) -> SortKey {
        let at = self.index_of(field);
        match field.value() {
            FieldValue::Amount => SortKey::Number(
                rows.iter()
                    .filter_map(|row| match &row[at] {
                        Cell::Amount(money) => Some(i128::from(money.minor_units())),
                        _ => None,
                    })
                    .sum(),
            ),
            FieldValue::Percent => SortKey::Number(
                rows.iter()
                    .filter_map(|row| match &row[at] {
                        Cell::Percent(Some(percent)) => Some(i128::from(percent.tenths())),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(i128::MIN),
            ),
            FieldValue::Day => rows
                .iter()
                .filter_map(|row| match &row[at] {
                    Cell::Day(day) => Some(*day),
                    _ => None,
                })
                .min()
                .map_or(SortKey::Number(0), SortKey::Day),
            FieldValue::Text => rows
                .iter()
                .filter_map(|row| match &row[at] {
                    Cell::Text(text) => Some(text.clone()),
                    _ => None,
                })
                .min()
                .map_or(SortKey::Number(0), SortKey::Text),
            FieldValue::YesNo => SortKey::Number(
                rows.iter()
                    .filter(|row| matches!(&row[at], Cell::YesNo(true)))
                    .count()
                    .try_into()
                    .unwrap_or(0),
            ),
        }
    }

    /// The keys of `encoding` over `rows`, in the order of the axis or of
    /// the legend: a day axis runs forward; otherwise the order of the rows
    /// (the dataset's own), unless the description sorts.
    fn ordered_keys(&self, encoding: &Encoding, rows: &[&[Cell]]) -> Result<Vec<Key>, CustomError> {
        let at = self.index_of(encoding.field());
        let unit = Self::unit_of(encoding);
        let mut groups: HashMap<Key, Vec<&[Cell]>> = HashMap::new();
        let mut keys = Vec::new();
        for row in rows {
            let key = Self::key(&row[at], unit)?;
            let group = groups.entry(key.clone()).or_default();
            if group.is_empty() {
                keys.push(key);
            }
            group.push(row);
        }
        match encoding.sort() {
            Some(Sort::Ascending) => keys.sort(),
            Some(Sort::Descending) => {
                keys.sort();
                keys.reverse();
            }
            Some(Sort::ByField(field)) => {
                keys.sort_by_cached_key(|key| {
                    groups
                        .get(key)
                        .map_or(SortKey::Number(0), |rows| self.sort_key(rows, field))
                });
            }
            None => {
                if encoding.kind() == FieldType::Temporal {
                    keys.sort();
                }
            }
        }
        Ok(keys)
    }

    // ---- values ----

    fn quantity_of(encoding: &Encoding) -> Quantity {
        if encoding.aggregate() == Some(Aggregate::Count) {
            Quantity::Count
        } else if encoding.field().value() == FieldValue::Percent {
            Quantity::Percent
        } else {
            Quantity::Amount
        }
    }

    /// The value of `encoding` for the rows of one place, exact. `None`
    /// when no row has a value (a percentage with nothing to compare with).
    fn value(&self, encoding: &Encoding, rows: &[&[Cell]]) -> Result<Option<i64>, CustomError> {
        let at = self.index_of(encoding.field());
        let overflow = CustomError::Chart(ChartError::Overflow);
        let count = || {
            i64::try_from(rows.len())
                .ok()
                .and_then(|count| count.checked_mul(100))
                .ok_or(CustomError::Chart(ChartError::Overflow))
        };
        if encoding.aggregate() == Some(Aggregate::Count) {
            return if rows.is_empty() {
                Ok(None)
            } else {
                Ok(Some(count()?))
            };
        }
        let values: Vec<i64> = rows
            .iter()
            .filter_map(|row| match &row[at] {
                Cell::Amount(money) => Some(money.minor_units()),
                Cell::Percent(Some(percent)) => Some(percent.tenths()),
                _ => None,
            })
            .collect();
        if values.is_empty() {
            return Ok(None);
        }
        let sum = || {
            values
                .iter()
                .try_fold(0_i64, |sum, value| sum.checked_add(*value))
                .ok_or(CustomError::Chart(ChartError::Overflow))
        };
        Ok(Some(match encoding.aggregate() {
            None => {
                if encoding.field().value() == FieldValue::Percent && values.len() > 1 {
                    // Ratios are not added up, and nothing is picked for the user.
                    return Err(CustomError::Invalid("bad-aggregate"));
                }
                sum()?
            }
            Some(Aggregate::Sum) => sum()?,
            Some(Aggregate::Mean) => {
                let count = i128::try_from(values.len()).map_err(|_| overflow)?;
                let mean: i128 = rounded(self.table.rounding(), i128::from(sum()?), count)
                    .ok_or(CustomError::Chart(ChartError::Overflow))?;
                i64::try_from(mean).map_err(|_| CustomError::Chart(ChartError::Overflow))?
            }
            Some(Aggregate::Min) => values.iter().copied().min().unwrap_or(0),
            Some(Aggregate::Max) => values.iter().copied().max().unwrap_or(0),
            Some(Aggregate::Count) => count()?,
        }))
    }

    /// A field of the rows of one place as words, for a `text` or `tooltip`
    /// channel: the value of an amount, or the distinct texts, days or answers
    /// in the order they come.
    fn words(&self, encoding: &Encoding, rows: &[&[Cell]]) -> Result<String, CustomError> {
        match encoding.field().value() {
            FieldValue::Amount | FieldValue::Percent => {
                let quantity = Self::quantity_of(encoding);
                Ok(self
                    .value(encoding, rows)?
                    .map(|value| self.label(quantity, value))
                    .unwrap_or_default())
            }
            _ => {
                if encoding.aggregate() == Some(Aggregate::Count) {
                    let count = i64::try_from(rows.len()).unwrap_or(i64::MAX);
                    return Ok(self.format.number(count.saturating_mul(100)));
                }
                let at = self.index_of(encoding.field());
                let unit = Self::unit_of(encoding);
                let mut seen: Vec<String> = Vec::new();
                for row in rows {
                    let words = match &row[at] {
                        Cell::Text(text) => text.clone(),
                        other => self.key_text(&Self::key(other, unit)?, unit),
                    };
                    if !seen.contains(&words) {
                        seen.push(words);
                    }
                }
                Ok(seen.join(", "))
            }
        }
    }

    /// The tooltip entries of one place, written after its value.
    fn tip(&self, channels: &Channels<'_>, rows: &[&[Cell]]) -> Result<String, CustomError> {
        let mut entries = Vec::new();
        for encoding in &channels.tooltips {
            let name = encoding.title().map_or_else(
                || encoding.field().name().to_owned(),
                |t| t.as_str().to_owned(),
            );
            entries.push(format!("{name}: {}", self.words(encoding, rows)?));
        }
        Ok(entries.join("; "))
    }

    // ---- layers ----

    /// The series of a layer: the values of `color` in order, or one series.
    fn series_keys(
        &self,
        channels: &Channels<'_>,
        rows: &[&[Cell]],
    ) -> Result<Vec<Option<Key>>, CustomError> {
        let Some(color) = channels.color else {
            return Ok(vec![None]);
        };
        let keys = self.ordered_keys(color, rows)?;
        if keys.len() > MAX_SERIES {
            return Err(CustomError::TooBig("series"));
        }
        Ok(keys.into_iter().map(Some).collect())
    }

    fn series_name(&self, channels: &Channels<'_>, key: Option<&Key>) -> String {
        match (key, channels.color) {
            (Some(key), Some(color)) => self.key_text(key, Self::unit_of(color)),
            _ => channels.default_name(),
        }
    }

    /// The groups of rows by (series, place); a row with no place is not on
    /// the axis and is left out.
    fn groups<'r>(
        &self,
        channels: &Channels<'_>,
        rows: &[&'r [Cell]],
        series: &[Option<Key>],
        places: &HashMap<Key, usize>,
    ) -> Result<Groups<'r>, CustomError> {
        let by_key: HashMap<&Key, usize> = series
            .iter()
            .enumerate()
            .filter_map(|(index, key)| key.as_ref().map(|key| (key, index)))
            .collect();
        let mut groups: HashMap<(usize, usize), Vec<&[Cell]>> = HashMap::new();
        for row in rows {
            let series_at = match channels.color {
                Some(color) => {
                    let key = Self::key(&row[self.index_of(color.field())], Self::unit_of(color))?;
                    *by_key.get(&key).ok_or(CustomError::Invalid("missing"))?
                }
                None => 0,
            };
            let place = match channels.x {
                Some(x) => {
                    let key = Self::key(&row[self.index_of(x.field())], Self::unit_of(x))?;
                    *places.get(&key).ok_or(CustomError::Invalid("missing"))?
                }
                None => 0,
            };
            groups.entry((series_at, place)).or_default().push(row);
        }
        Ok(groups)
    }

    /// The layers with a value at some places (lines, bars, areas, points,
    /// texts).
    fn aligned(
        &self,
        channels: &Channels<'_>,
        rows: &[&[Cell]],
        places: &HashMap<Key, usize>,
        count: usize,
        first_color: &mut usize,
        paint: &Paint<'_>,
    ) -> Result<(Vec<Aligned>, Quantity), CustomError> {
        let y = channels.y.ok_or(CustomError::Invalid("missing"))?;
        let quantity = Self::quantity_of(y);
        let series = self.series_keys(channels, rows)?;
        // A chart draws at most as many points as it may use rows: every
        // series has a point at every place.
        if count.saturating_mul(series.len()) > MAX_ROWS {
            return Err(CustomError::TooBig("rows"));
        }
        let groups = self.groups(channels, rows, &series, places)?;
        let mut built = Vec::with_capacity(series.len());
        for (index, key) in series.iter().enumerate() {
            let mut values = vec![None; count];
            let mut labels = vec![NOTHING.to_owned(); count];
            let mut texts = vec![String::new(); count];
            let mut raw_sizes: Vec<Option<i64>> = vec![None; count];
            for place in 0..count {
                let Some(group) = groups.get(&(index, place)) else {
                    continue;
                };
                let Some(value) = self.value(y, group)? else {
                    continue;
                };
                values[place] = Some(value);
                let tip = self.tip(channels, group)?;
                let label = self.label(quantity, value);
                labels[place] = if tip.is_empty() {
                    label
                } else {
                    format!("{label} ({tip})")
                };
                if let Some(text) = channels.text {
                    texts[place] = self.words(text, group)?;
                }
                if let Some(size) = channels.size {
                    raw_sizes[place] = self.value(size, group)?;
                }
            }
            built.push(Aligned {
                name: self.series_name(channels, key.as_ref()),
                color: paint.pick(*first_color + index),
                values,
                labels,
                texts,
                raw_sizes,
                sizes: Vec::new(),
            });
        }
        // The next layer's series take the colors after this layer's; a
        // layer in a color of its own takes none of them.
        if paint.own.is_none() {
            *first_color += series.len();
        }
        // The sizes are tiers over the whole layer, not per series.
        let all: Vec<Option<i64>> = built
            .iter()
            .flat_map(|one| one.raw_sizes.iter().copied())
            .collect();
        let tiers = dot_sizes(&all);
        for (index, one) in built.iter_mut().enumerate() {
            one.sizes = tiers[index * count..(index + 1) * count].to_vec();
        }
        Ok((built, quantity))
    }

    /// Rules across the chart at a level (`y` only).
    fn levels(
        &self,
        channels: &Channels<'_>,
        rows: &[&[Cell]],
        first_color: &mut usize,
        paint: &Paint<'_>,
    ) -> Result<(Vec<Levels>, Quantity), CustomError> {
        let y = channels.y.ok_or(CustomError::Invalid("missing"))?;
        let quantity = Self::quantity_of(y);
        let series = self.series_keys(channels, rows)?;
        let at_color = channels.color.map(|color| self.index_of(color.field()));
        let mut built = Vec::with_capacity(series.len());
        for (index, key) in series.iter().enumerate() {
            let mine: Vec<&[Cell]> = match (key, channels.color, at_color) {
                (Some(key), Some(color), Some(at)) => {
                    let unit = Self::unit_of(color);
                    let mut mine = Vec::new();
                    for row in rows {
                        if &Self::key(&row[at], unit)? == key {
                            mine.push(*row);
                        }
                    }
                    mine
                }
                _ => rows.to_vec(),
            };
            let mut levels: Vec<i64> = Vec::new();
            if y.aggregate().is_some() {
                levels.extend(self.value(y, &mine)?);
            } else {
                for row in &mine {
                    levels.extend(self.value(y, std::slice::from_ref(row))?);
                }
                levels.sort_unstable();
                levels.dedup();
            }
            built.push((
                self.series_name(channels, key.as_ref()),
                paint.pick(*first_color + index),
                levels
                    .into_iter()
                    .map(|level| (level, self.label(quantity, level)))
                    .collect(),
            ));
        }
        if paint.own.is_none() {
            *first_color += series.len();
        }
        Ok((built, quantity))
    }

    /// Rules at places (`x` only).
    fn verticals(
        &self,
        channels: &Channels<'_>,
        rows: &[&[Cell]],
        places: &HashMap<Key, usize>,
        first_color: &mut usize,
        paint: &Paint<'_>,
    ) -> Result<Vec<(String, u8, Vec<usize>)>, CustomError> {
        let series = self.series_keys(channels, rows)?;
        let groups = self.groups(channels, rows, &series, places)?;
        let mut built = Vec::with_capacity(series.len());
        for (index, key) in series.iter().enumerate() {
            let mut at: Vec<usize> = groups
                .keys()
                .filter(|(series_at, _)| *series_at == index)
                .map(|(_, place)| *place)
                .collect();
            at.sort_unstable();
            built.push((
                self.series_name(channels, key.as_ref()),
                paint.pick(*first_color + index),
                at,
            ));
        }
        if paint.own.is_none() {
            *first_color += series.len();
        }
        Ok(built)
    }

    // ---- the whole chart ----

    fn render(&self, spec: &ChartSpec) -> Result<Option<CustomChart>, CustomError> {
        let palette = Palette::of(spec);
        let mut layer_rows = Vec::new();
        let mut channels: Vec<Channels<'_>> = Vec::new();
        for layer in spec.layers() {
            layer_rows.push(self.rows_of(spec, layer)?);
            let below = channels.last();
            let these = Channels::of(layer, below);
            channels.push(these);
        }
        if layer_rows.iter().all(Vec::is_empty) {
            return Ok(None);
        }
        let layers = spec.layers();
        let currency = self.table.currency().to_string();
        let words = |text: Option<&kassensturz_report::spec::SpecText>| {
            text.map(|text| text.as_str().to_owned())
        };
        let size = spec.size();
        let wrap = |chart: ChartModel| CustomChart {
            chart,
            title: words(spec.title()),
            description: words(spec.description()),
            width: size.and_then(|size| size.width()),
            height: size.and_then(|size| size.height()),
        };

        // A pie or donut stands alone (the model checked that).
        if let Some(layer) = layers.iter().find(|l| l.mark().kind() == MarkKind::Arc) {
            let at = layers
                .iter()
                .position(|l| std::ptr::eq(l, layer))
                .unwrap_or(0);
            return self
                .round(spec, layer, &channels[at], &layer_rows[at])
                .map(|chart| Some(wrap(chart)));
        }

        // The category axis: the places of every layer that has an `x`.
        let x_layers: Vec<usize> = (0..layers.len())
            .filter(|&i| channels[i].x.is_some())
            .collect();
        let (places, place_list, x_unit) = if let Some(&first) = x_layers.first() {
            let x = channels[first].x.ok_or(CustomError::Invalid("missing"))?;
            let all_rows: Vec<&[Cell]> = x_layers
                .iter()
                .flat_map(|&i| layer_rows[i].iter().copied())
                .collect();
            let keys = self.ordered_keys(x, &all_rows)?;
            let index: HashMap<Key, usize> = keys
                .iter()
                .cloned()
                .enumerate()
                .map(|(at, key)| (key, at))
                .collect();
            (index, keys, Self::unit_of(x))
        } else {
            return Err(CustomError::Invalid("nothing-to-draw"));
        };
        let count = place_list.len();
        let categories: Vec<String> = place_list
            .iter()
            .map(|key| self.key_text(key, x_unit))
            .collect();
        let unsorted = place_list
            .iter()
            .position(|key| matches!(key, Key::Text(text) if text.is_empty()));

        // The base: the first line, bar or area layer.
        let base_at = layers.iter().position(|l| {
            matches!(
                l.mark().kind(),
                MarkKind::Line | MarkKind::Bar | MarkKind::Area
            )
        });
        let mut first_color = 0_usize;
        let mut quantity: Option<Quantity> = None;
        let mut take_quantity = |found: Quantity| -> Result<(), CustomError> {
            match quantity {
                Some(own) if own != found => Err(CustomError::Invalid("layers-differ")),
                _ => {
                    quantity = Some(found);
                    Ok(())
                }
            }
        };
        let mut kind = ChartKind::Bar;
        let mut series: Vec<Series> = Vec::new();
        let mut overlays: Vec<Overlay> = Vec::new();
        let mut table_rows: Vec<Vec<String>> = Vec::new();

        // The base is built first so its series take the first colors.
        let mut order: Vec<usize> = (0..layers.len()).collect();
        if let Some(base) = base_at {
            order.retain(|&i| i != base);
            order.insert(0, base);
        }
        for at in order {
            let layer = &layers[at];
            let mark = layer.mark();
            let rows = &layer_rows[at];
            let is_base = Some(at) == base_at;
            // A mark that names its color is drawn in it when the layer has
            // no `color` channel; with one, the channel's series decide.
            let paint = Paint {
                palette: &palette,
                own: if channels[at].color.is_none() {
                    mark.color().map(|color| palette.index_of(color))
                } else {
                    None
                },
            };
            match mark.kind() {
                MarkKind::Rule if channels[at].x.is_none() => {
                    let (levels, own) =
                        self.levels(&channels[at], rows, &mut first_color, &paint)?;
                    take_quantity(own)?;
                    for (name, _, items) in &levels {
                        for (_, label) in items {
                            table_rows.push(vec![String::new(), name.clone(), label.clone()]);
                        }
                    }
                    overlays.push(Overlay {
                        mark: OverlayMark::LevelRule,
                        series: levels
                            .into_iter()
                            .map(|(name, color, items)| OverlaySeries {
                                name,
                                color,
                                items: items
                                    .into_iter()
                                    .map(|(level, label)| OverlayItem {
                                        category: None,
                                        minor_units: Some(level),
                                        label,
                                        text: String::new(),
                                        size: 1,
                                    })
                                    .collect(),
                            })
                            .collect(),
                    });
                }
                MarkKind::Rule => {
                    let lines =
                        self.verticals(&channels[at], rows, &places, &mut first_color, &paint)?;
                    for (name, _, at) in &lines {
                        for place in at {
                            table_rows.push(vec![
                                categories[*place].clone(),
                                name.clone(),
                                String::new(),
                            ]);
                        }
                    }
                    overlays.push(Overlay {
                        mark: OverlayMark::CategoryRule,
                        series: lines
                            .into_iter()
                            .map(|(name, color, at)| OverlaySeries {
                                name,
                                color,
                                items: at
                                    .into_iter()
                                    .map(|place| OverlayItem {
                                        category: Some(place),
                                        minor_units: None,
                                        label: categories[place].clone(),
                                        text: String::new(),
                                        size: 1,
                                    })
                                    .collect(),
                            })
                            .collect(),
                    });
                }
                _ => {
                    let (built, own) = self.aligned(
                        &channels[at],
                        rows,
                        &places,
                        count,
                        &mut first_color,
                        &paint,
                    )?;
                    take_quantity(own)?;
                    // Every layer's table rows: the place, the series, the value.
                    for one in &built {
                        for (place, name) in categories.iter().enumerate() {
                            if one.values[place].is_some() {
                                table_rows.push(vec![
                                    name.clone(),
                                    one.name.clone(),
                                    one.labels[place].clone(),
                                ]);
                            }
                        }
                    }
                    if is_base {
                        let stacked = !built.is_empty()
                            && channels[at].color.is_some()
                            && matches!(mark.kind(), MarkKind::Bar | MarkKind::Area);
                        kind = match (mark.kind(), stacked) {
                            (MarkKind::Line, _) => ChartKind::Line,
                            (MarkKind::Area, true) => ChartKind::StackedArea,
                            (MarkKind::Area, false) => ChartKind::Area,
                            (_, true) => ChartKind::StackedBar,
                            (_, false) => ChartKind::Bar,
                        };
                        series = built
                            .iter()
                            .map(|one| Series {
                                key: "series",
                                name: Some(one.name.clone()),
                                color: one.color,
                                points: (0..count)
                                    .map(|place| Point {
                                        minor_units: one.values[place].unwrap_or(0),
                                        label: one.labels[place].clone(),
                                    })
                                    .collect(),
                            })
                            .collect();
                        if mark.point() {
                            overlays.push(overlay_of(OverlayMark::Point, &built, count));
                        }
                    } else {
                        let kind = match mark.kind() {
                            MarkKind::Line => OverlayMark::Line,
                            MarkKind::Area => OverlayMark::Area,
                            MarkKind::Bar => OverlayMark::Bar,
                            MarkKind::Text => OverlayMark::Text,
                            _ => OverlayMark::Point,
                        };
                        overlays.push(overlay_of(kind, &built, count));
                    }
                }
            }
        }
        let quantity = quantity.unwrap_or(Quantity::Amount);

        // The axis reaches over every value of every layer.
        let stacked = matches!(kind, ChartKind::StackedBar | ChartKind::StackedArea);
        let (mut low, mut high) = extent(&series, stacked);
        for item in overlays
            .iter()
            .flat_map(|overlay| &overlay.series)
            .flat_map(|series| &series.items)
            .filter_map(|item| item.minor_units)
        {
            low = low.min(item);
            high = high.max(item);
        }
        let ticks: Vec<Tick> = match quantity {
            Quantity::Amount => ticks(
                low,
                high,
                major_unit(self.table.currency()),
                self.table.currency(),
                self.format,
            )?,
            Quantity::Count => ticks_with(low, high, 100, |at| self.format.number(at))?,
            Quantity::Percent => ticks_with(low, high, 10, |at| {
                self.format.percent(Percent::from_tenths(at))
            })?,
        };

        Ok(Some(wrap(ChartModel {
            kind,
            dataset: spec.dataset().id(),
            currency: currency.clone(),
            categories,
            unsorted,
            series,
            ticks,
            total: String::new(),
            table: TextTable {
                currency,
                columns: vec!["category", "series", "value"],
                rows: table_rows,
            },
            slices: Slices::default(),
            hidden: None,
            overlays,
            extra_colors: palette.extras.clone(),
            slice_colors: Vec::new(),
        })))
    }

    /// A pie or donut: one slice for each value of `color`, its size the
    /// `theta` of the rows with that value.
    fn round(
        &self,
        spec: &ChartSpec,
        layer: &Layer,
        channels: &Channels<'_>,
        rows: &[&[Cell]],
    ) -> Result<ChartModel, CustomError> {
        let theta = channels.theta.ok_or(CustomError::Invalid("missing"))?;
        let color = channels.color.ok_or(CustomError::Invalid("missing"))?;
        let quantity = Self::quantity_of(theta);
        let keys = self.series_keys(channels, rows)?;
        let at = self.index_of(color.field());
        let unit = Self::unit_of(color);
        let mut slices = Vec::with_capacity(keys.len());
        for key in keys.into_iter().flatten() {
            let mut mine = Vec::new();
            for row in rows {
                if Self::key(&row[at], unit)? == key {
                    mine.push(*row);
                }
            }
            let Some(value) = self.value(theta, &mine)? else {
                continue;
            };
            let name = self.key_text(&key, unit);
            let nothing_yet = key == Key::Text(String::new());
            let tip = self.tip(channels, &mine)?;
            let label = self.label(quantity, value);
            slices.push(ShownRow {
                unsorted: nothing_yet,
                name,
                minor_units: value,
                label: if tip.is_empty() {
                    label
                } else {
                    format!("{label} ({tip})")
                },
            });
        }
        // Largest first, as the built-in pies have them; equal slices keep
        // the order of the data.
        if color.sort().is_none() {
            slices.sort_by_key(|slice| std::cmp::Reverse(slice.minor_units));
        }
        let shown = round_slices(slices, self.format, |value| self.label(quantity, value))?;
        // The slices take the chart's own colors in order, else the palette's.
        let palette = Palette::of(spec);
        let slice_colors: Vec<u8> = (0..shown.rows.len()).map(|at| palette.color(at)).collect();
        let inner = layer.mark().inner_radius().is_some();
        let currency = self.table.currency().to_string();
        let points: Vec<Point> = shown
            .rows
            .iter()
            .map(|row| Point {
                minor_units: row.minor_units,
                label: row.label.clone(),
            })
            .collect();
        Ok(ChartModel {
            kind: if inner {
                ChartKind::Donut
            } else {
                ChartKind::Pie
            },
            dataset: spec.dataset().id(),
            currency: currency.clone(),
            categories: shown.rows.iter().map(|row| row.name.clone()).collect(),
            unsorted: shown.rows.iter().position(|row| row.unsorted),
            series: vec![Series {
                key: "series",
                name: None,
                color: 0,
                points,
            }],
            ticks: Vec::new(),
            total: String::new(),
            table: TextTable {
                currency,
                columns: vec!["category", "total"],
                rows: shown
                    .rows
                    .iter()
                    .map(|row| vec![row.name.clone(), row.label.clone()])
                    .collect(),
            },
            slices: shown.slices,
            hidden: None,
            overlays: Vec::new(),
            extra_colors: palette.extras,
            slice_colors,
        })
    }
}

/// The overlay of the series of an aligned layer: an item at every place
/// that has a value.
fn overlay_of(mark: OverlayMark, built: &[Aligned], count: usize) -> Overlay {
    Overlay {
        mark,
        series: built
            .iter()
            .map(|one| OverlaySeries {
                name: one.name.clone(),
                color: one.color,
                items: (0..count)
                    .filter_map(|place| {
                        Some(OverlayItem {
                            category: Some(place),
                            minor_units: Some(one.values[place]?),
                            label: one.labels[place].clone(),
                            text: one.texts[place].clone(),
                            size: one.sizes[place],
                        })
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The sizes of the dots of one series from their raw values: 1 for the
/// smallest, [`MAX_DOT_SIZE`] for the largest, in whole steps by integer
/// arithmetic. Without a `size` channel every dot is the middle size.
fn dot_sizes(raw: &[Option<i64>]) -> Vec<u8> {
    let known: Vec<i64> = raw.iter().flatten().copied().collect();
    let (Some(&low), Some(&high)) = (known.iter().min(), known.iter().max()) else {
        return vec![2; raw.len()];
    };
    raw.iter()
        .map(|value| match value {
            Some(value) if high > low => {
                let steps = i128::from(MAX_DOT_SIZE) - 1;
                let at = i128::from(*value - low) * steps / i128::from(high - low);
                u8::try_from(1 + at).unwrap_or(MAX_DOT_SIZE)
            }
            _ => 2,
        })
        .collect()
}

/// `a ÷ b` rounded by the book's mode.
fn rounded(mode: RoundingMode, numerator: i128, denominator: i128) -> Option<i128> {
    mode.divide(numerator, denominator)
}

/// Exact order of `left` (with `left_places` decimal places) against `right`
/// (with `right_places`), by scaling the one with fewer places.
fn scaled(left: i128, left_places: u8, right: i128, right_places: u8) -> Ordering {
    let places = left_places.max(right_places);
    let scale = |value: i128, own: u8| value * 10_i128.pow(u32::from(places - own));
    scale(left, left_places).cmp(&scale(right, right_places))
}

/// The colors of a chart as palette indices: the description's own colors in
/// order, else the standard ones (D-283). A color the description names as
/// `#rrggbb` has no place in the app's palette: it gets an index after the
/// eight places, which `extras` explains (`ChartModel::extra_colors`).
struct Palette {
    /// The index of each color of `config.range.category`, in order.
    chosen: Vec<u8>,
    /// The free colors, in order of first use; extra `n` has index 8 + `n`.
    extras: Vec<Rgb>,
}

impl Palette {
    fn of(spec: &ChartSpec) -> Self {
        let mut palette = Self {
            chosen: Vec::new(),
            extras: Vec::new(),
        };
        let listed: Vec<u8> = spec
            .palette()
            .iter()
            .map(|color| palette.add(*color))
            .collect();
        palette.chosen = listed;
        // Every mark's own color is known before any series is built.
        for layer in spec.layers() {
            if let Some(color) = layer.mark().color() {
                palette.add(color);
            }
        }
        palette
    }

    /// Adds a free color once, and returns the index of `color`.
    fn add(&mut self, color: SpecColor) -> u8 {
        if let SpecColor::Rgb(rgb) = color
            && !self.extras.contains(&rgb)
        {
            self.extras.push(rgb);
        }
        self.index_of(color)
    }

    /// The index of a color that [`add`](Self::add) has seen. A free color
    /// that was not added reads as the first place.
    fn index_of(&self, color: SpecColor) -> u8 {
        match color {
            SpecColor::Slot(name) => u8::try_from(name.index()).unwrap_or(0),
            SpecColor::Rgb(rgb) => self
                .extras
                .iter()
                .position(|known| *known == rgb)
                .and_then(|at| u8::try_from(at).ok())
                .map_or(0, |at| PALETTE_SIZE.saturating_add(at)),
        }
    }

    /// The color of series number `series` of the chart.
    fn color(&self, series: usize) -> u8 {
        if self.chosen.is_empty() {
            ChartModel::color_of(series)
        } else {
            self.chosen[series % self.chosen.len()]
        }
    }
}

/// The colors a layer draws its series in: the chart's palette, and the
/// color its mark names, if it names one and has no `color` channel.
struct Paint<'a> {
    palette: &'a Palette,
    own: Option<u8>,
}

impl Paint<'_> {
    /// The color of a series: the layer's own if it has one, else the next
    /// of the chart's.
    fn pick(&self, series: usize) -> u8 {
        self.own.unwrap_or_else(|| self.palette.color(series))
    }
}
