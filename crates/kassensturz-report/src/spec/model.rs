// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chart model (D-502). Fields are private and read through accessors;
//! the constructors enforce the rules of the design, so the parser (M6e.1)
//! and the tests share them.

use kassensturz_core::Date;

use super::color::SpecColor;
use super::fields::{FieldValue, SpecDataset, SpecField};

/// The version of the subset this program reads (`"kassensturz":
/// {"vl_subset": 1}`, D-280).
pub const VL_SUBSET: u32 = 1;
/// Most layers in one chart (D-281).
pub const MAX_LAYERS: usize = 4;
/// Most encodings over the whole description, tooltip entries one by one.
pub const MAX_ENCODINGS: usize = 8;
/// Most filters over the whole description, the layers' own included.
pub const MAX_FILTERS: usize = 8;
/// Most values in a `oneOf` filter.
pub const MAX_ONE_OF: usize = 50;
/// Most palette colors.
pub const MAX_PALETTE: usize = 8;
/// Most characters in a text of a description.
pub const MAX_TEXT_CHARS: usize = 200;
/// Smallest width or height.
pub const MIN_SIZE: u16 = 100;
/// Largest width or height.
pub const MAX_SIZE: u16 = 2000;
/// Most decimal places of a [`SpecNumber`].
const MAX_PLACES: u8 = 18;

/// Why a value of the model can't be built. The words are those of the error
/// codes in `docs/REPORT_DESIGNER_M6E.md` section 5, so the parser maps them
/// one to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelError {
    /// `report.spec-invalid` with this reason.
    Invalid(&'static str),
    /// `report.spec-too-big` with this limit.
    TooBig(&'static str),
}

/// Text from a description: at most 200 characters, one line, no control
/// characters. It is data, always shown as plain text (T-08, T-21).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecText(String);

impl SpecText {
    /// Checks and keeps `text`.
    ///
    /// # Errors
    ///
    /// `TooBig("label")` for more than 200 characters, `Invalid("bad-text")`
    /// for a control character (a line break included).
    pub fn new(text: &str) -> Result<Self, ModelError> {
        if text.chars().count() > MAX_TEXT_CHARS {
            return Err(ModelError::TooBig("label"));
        }
        if text.chars().any(char::is_control) {
            return Err(ModelError::Invalid("bad-text"));
        }
        Ok(Self(text.to_owned()))
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A number from a description: digits and decimal places read from the
/// text, never through floating point (D-506). 12.50 is digits 1250, places 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecNumber {
    digits: i64,
    places: u8,
}

impl SpecNumber {
    /// A whole number.
    #[must_use]
    pub const fn whole(value: i64) -> Self {
        Self {
            digits: value,
            places: 0,
        }
    }

    /// Reads `-?digits(.digits)?`; nothing else (no exponent, no sign
    /// plus, no spaces).
    ///
    /// # Errors
    ///
    /// `Invalid("bad-number")` for any other shape, more than 18 places, or a
    /// value that does not fit.
    pub fn parse(text: &str) -> Result<Self, ModelError> {
        let bad = ModelError::Invalid("bad-number");
        let (negative, rest) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = match rest.split_once('.') {
            Some((whole, fraction)) => (whole, fraction),
            None => (rest, ""),
        };
        let all_digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        if whole.is_empty()
            || !all_digits(whole)
            || !all_digits(fraction)
            || (rest.contains('.') && fraction.is_empty())
        {
            return Err(bad);
        }
        let places = u8::try_from(fraction.len()).map_err(|_| bad)?;
        if places > MAX_PLACES {
            return Err(bad);
        }
        let sign = if negative { "-" } else { "" };
        let joined = format!("{sign}{whole}{fraction}");
        let digits = joined.parse::<i64>().map_err(|_| bad)?;
        Ok(Self { digits, places })
    }

    /// The digits without the decimal point.
    #[must_use]
    pub const fn digits(self) -> i64 {
        self.digits
    }

    /// The decimal places as written.
    #[must_use]
    pub const fn places(self) -> u8 {
        self.places
    }

    /// The number in minor units of a currency with `decimals` places, or
    /// `None` when it has finer places than that (it is never rounded) or
    /// does not fit.
    #[must_use]
    pub fn to_minor_units(self, decimals: u8) -> Option<i64> {
        let value = i128::from(self.digits);
        let scaled = if decimals >= self.places {
            value.checked_mul(10_i128.checked_pow(u32::from(decimals - self.places))?)?
        } else {
            let divisor = 10_i128.checked_pow(u32::from(self.places - decimals))?;
            if value % divisor != 0 {
                return None;
            }
            value / divisor
        };
        i64::try_from(scaled).ok()
    }

    /// Exact order against another number.
    #[must_use]
    pub fn compare(self, other: Self) -> std::cmp::Ordering {
        let places = self.places.max(other.places);
        let scale = |number: Self| {
            i128::from(number.digits) * 10_i128.pow(u32::from(places - number.places))
        };
        scale(self).cmp(&scale(other))
    }
}

/// The eight colors of the chart palette, by the fixed words of
/// `ChartPalette.qml` (Okabe-Ito order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteName {
    /// Blue.
    Blue,
    /// Vermillion.
    Vermillion,
    /// Bluish green.
    BluishGreen,
    /// Reddish purple.
    ReddishPurple,
    /// Orange.
    Orange,
    /// Sky blue.
    SkyBlue,
    /// Yellow.
    Yellow,
    /// Grey.
    Grey,
}

impl PaletteName {
    /// All eight, in palette order.
    pub const ALL: [Self; 8] = [
        Self::Blue,
        Self::Vermillion,
        Self::BluishGreen,
        Self::ReddishPurple,
        Self::Orange,
        Self::SkyBlue,
        Self::Yellow,
        Self::Grey,
    ];

    /// The word a description writes.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Vermillion => "vermillion",
            Self::BluishGreen => "bluish-green",
            Self::ReddishPurple => "reddish-purple",
            Self::Orange => "orange",
            Self::SkyBlue => "sky-blue",
            Self::Yellow => "yellow",
            Self::Grey => "grey",
        }
    }

    /// The color with this word.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|name| name.word() == word)
    }

    /// The position in the palette, which is the color's index in
    /// `ChartPalette.colors`.
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|name| *name == self).unwrap_or(0)
    }
}

/// Width and height of a chart, each 100 to 2000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartSize {
    width: Option<u16>,
    height: Option<u16>,
}

impl ChartSize {
    /// Checks both.
    ///
    /// # Errors
    ///
    /// `Invalid("bad-number")` for a side outside 100 to 2000.
    pub fn new(width: Option<u16>, height: Option<u16>) -> Result<Self, ModelError> {
        let fine = |side: Option<u16>| side.is_none_or(|v| (MIN_SIZE..=MAX_SIZE).contains(&v));
        if fine(width) && fine(height) {
            Ok(Self { width, height })
        } else {
            Err(ModelError::Invalid("bad-number"))
        }
    }

    /// The width, when given.
    #[must_use]
    pub const fn width(self) -> Option<u16> {
        self.width
    }

    /// The height, when given.
    #[must_use]
    pub const fn height(self) -> Option<u16> {
        self.height
    }
}

/// What a mark draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// A line.
    Line,
    /// Bars.
    Bar,
    /// An area.
    Area,
    /// Slices of a pie or donut.
    Arc,
    /// Points.
    Point,
    /// Text labels.
    Text,
    /// A level line or a line at one date.
    Rule,
}

/// A mark with its options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    kind: MarkKind,
    tooltip: bool,
    point: bool,
    inner_radius: Option<u16>,
    color: Option<SpecColor>,
}

impl Mark {
    /// Checks the options against the kind: `point` only on a line or area,
    /// `inner_radius` (above 0) only on an arc.
    ///
    /// # Errors
    ///
    /// `Invalid("bad-channel-for-mark")` for an option the kind doesn't take,
    /// `Invalid("bad-number")` for a radius of 0.
    pub fn new(
        kind: MarkKind,
        tooltip: bool,
        point: bool,
        inner_radius: Option<u16>,
    ) -> Result<Self, ModelError> {
        if point && !matches!(kind, MarkKind::Line | MarkKind::Area) {
            return Err(ModelError::Invalid("bad-channel-for-mark"));
        }
        if inner_radius.is_some() && kind != MarkKind::Arc {
            return Err(ModelError::Invalid("bad-channel-for-mark"));
        }
        if inner_radius == Some(0) {
            return Err(ModelError::Invalid("bad-number"));
        }
        Ok(Self {
            kind,
            tooltip,
            point,
            inner_radius,
            color: None,
        })
    }

    /// The same mark drawn in `color` (M7a.10). It is used by a layer that
    /// has no `color` channel; with one, the channel's colors are used.
    #[must_use]
    pub const fn with_color(mut self, color: SpecColor) -> Self {
        self.color = Some(color);
        self
    }

    /// A mark without options.
    #[must_use]
    pub const fn plain(kind: MarkKind) -> Self {
        Self {
            kind,
            tooltip: false,
            point: false,
            inner_radius: None,
            color: None,
        }
    }

    /// The kind.
    #[must_use]
    pub const fn kind(self) -> MarkKind {
        self.kind
    }

    /// Whether the mark shows a tooltip.
    #[must_use]
    pub const fn tooltip(self) -> bool {
        self.tooltip
    }

    /// Whether a line or area also marks its points.
    #[must_use]
    pub const fn point(self) -> bool {
        self.point
    }

    /// The inner radius that makes an arc a donut.
    #[must_use]
    pub const fn inner_radius(self) -> Option<u16> {
        self.inner_radius
    }

    /// The color the mark itself names, if it does (M7a.10).
    #[must_use]
    pub const fn color(self) -> Option<SpecColor> {
        self.color
    }
}

/// Where a field goes on the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Horizontal position.
    X,
    /// Vertical position.
    Y,
    /// Color, one series per value.
    Color,
    /// Size of a point.
    Size,
    /// Angle of a slice.
    Theta,
    /// The label of a text mark.
    Text,
    /// A tooltip entry.
    Tooltip,
}

/// How a field is read (`type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Names.
    Nominal,
    /// Names in an order.
    Ordinal,
    /// A quantity.
    Quantitative,
    /// A day.
    Temporal,
}

/// An aggregate over rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    /// Exact sum.
    Sum,
    /// Exact sum divided by the count, rounded by the book's rule (D-027).
    Mean,
    /// Smallest.
    Min,
    /// Largest.
    Max,
    /// Number of rows.
    Count,
}

/// A time unit on a day field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    /// By year.
    Year,
    /// By month.
    YearMonth,
    /// By day.
    YearMonthDate,
}

/// How an encoding is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// Ascending.
    Ascending,
    /// Descending.
    Descending,
    /// By the values of another field.
    ByField(&'static SpecField),
}

/// One field on one channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoding {
    channel: Channel,
    field: &'static SpecField,
    kind: FieldType,
    title: Option<SpecText>,
    aggregate: Option<Aggregate>,
    time_unit: Option<TimeUnit>,
    sort: Option<Sort>,
}

impl Encoding {
    /// A plain encoding of `field` as `kind` on `channel`, checked by
    /// [`Encoding::with`] rules.
    ///
    /// # Errors
    ///
    /// As [`Encoding::new`].
    pub fn plain(
        channel: Channel,
        field: &'static SpecField,
        kind: FieldType,
    ) -> Result<Self, ModelError> {
        Self::new(channel, field, kind, None, None, None, None)
    }

    /// Checks the encoding: the type must suit the field (`type-mismatch`;
    /// a quantity for any field only with `count`), the channel must take the
    /// type, `sum` and `mean` take no percent and no non-quantity
    /// (`bad-aggregate`), and a time unit needs a temporal day field.
    ///
    /// # Errors
    ///
    /// `Invalid("type-mismatch")` or `Invalid("bad-aggregate")`.
    pub fn new(
        channel: Channel,
        field: &'static SpecField,
        kind: FieldType,
        title: Option<SpecText>,
        aggregate: Option<Aggregate>,
        time_unit: Option<TimeUnit>,
        sort: Option<Sort>,
    ) -> Result<Self, ModelError> {
        let counted = aggregate == Some(Aggregate::Count);
        if !field.value().suits(kind, counted) {
            return Err(ModelError::Invalid("type-mismatch"));
        }
        let kind_fits_channel = match channel {
            Channel::X => kind != FieldType::Quantitative || counted,
            Channel::Y | Channel::Theta | Channel::Size => kind == FieldType::Quantitative,
            Channel::Color => matches!(kind, FieldType::Nominal | FieldType::Ordinal),
            Channel::Text | Channel::Tooltip => true,
        };
        if !kind_fits_channel {
            return Err(ModelError::Invalid("type-mismatch"));
        }
        match aggregate {
            Some(Aggregate::Sum | Aggregate::Mean) => {
                if !matches!(field.value(), FieldValue::Amount) {
                    return Err(ModelError::Invalid("bad-aggregate"));
                }
            }
            Some(Aggregate::Min | Aggregate::Max) => {
                if !matches!(field.value(), FieldValue::Amount | FieldValue::Percent) {
                    return Err(ModelError::Invalid("bad-aggregate"));
                }
            }
            Some(Aggregate::Count) | None => {}
        }
        if time_unit.is_some() && !(kind == FieldType::Temporal && field.value() == FieldValue::Day)
        {
            return Err(ModelError::Invalid("type-mismatch"));
        }
        Ok(Self {
            channel,
            field,
            kind,
            title,
            aggregate,
            time_unit,
            sort,
        })
    }

    /// The channel.
    #[must_use]
    pub const fn channel(&self) -> Channel {
        self.channel
    }

    /// The field.
    #[must_use]
    pub const fn field(&self) -> &'static SpecField {
        self.field
    }

    /// The type.
    #[must_use]
    pub const fn kind(&self) -> FieldType {
        self.kind
    }

    /// The title, when given.
    #[must_use]
    pub const fn title(&self) -> Option<&SpecText> {
        self.title.as_ref()
    }

    /// The aggregate, when given.
    #[must_use]
    pub const fn aggregate(&self) -> Option<Aggregate> {
        self.aggregate
    }

    /// The time unit, when given.
    #[must_use]
    pub const fn time_unit(&self) -> Option<TimeUnit> {
        self.time_unit
    }

    /// The sort, when given.
    #[must_use]
    pub const fn sort(&self) -> Option<Sort> {
        self.sort
    }
}

/// A value in a filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecValue {
    /// Text.
    Text(SpecText),
    /// A calendar day.
    Day(Date),
    /// A number.
    Number(SpecNumber),
    /// Yes or no (JSON `true` or `false`, Q-358).
    YesNo(bool),
}

/// What a filter tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterTest {
    /// Equal to one value.
    Equal(SpecValue),
    /// One of up to 50 values.
    OneOf(Vec<SpecValue>),
    /// Within a range, both ends included; `None` is open.
    Range {
        /// The lower end.
        from: Option<SpecValue>,
        /// The upper end.
        to: Option<SpecValue>,
    },
}

/// A filter on one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFilter {
    field: &'static SpecField,
    test: FilterTest,
}

impl FieldFilter {
    /// Checks the values against the field: text for a text field, days for a
    /// day field, numbers for an amount or percent; a yes/no field takes
    /// `true` or `false` (Q-358), by equal or one-of only. A `OneOf` has 1 to 50 values and a range has
    /// at least one end, both of one kind and in order.
    ///
    /// # Errors
    ///
    /// `Invalid("type-mismatch")`, `Invalid("missing")`,
    /// `Invalid("bad-choice")` or `TooBig("values")`.
    pub fn new(field: &'static SpecField, test: FilterTest) -> Result<Self, ModelError> {
        let suits = |value: &SpecValue| {
            matches!(
                (field.value(), value),
                (FieldValue::Text, SpecValue::Text(_))
                    | (FieldValue::Day, SpecValue::Day(_))
                    | (FieldValue::YesNo, SpecValue::YesNo(_))
                    | (
                        FieldValue::Amount | FieldValue::Percent,
                        SpecValue::Number(_)
                    )
            )
        };
        let mismatch = ModelError::Invalid("type-mismatch");
        match &test {
            FilterTest::Equal(value) => {
                if !suits(value) {
                    return Err(mismatch);
                }
            }
            FilterTest::OneOf(values) => {
                if values.is_empty() {
                    return Err(ModelError::Invalid("missing"));
                }
                if values.len() > MAX_ONE_OF {
                    return Err(ModelError::TooBig("values"));
                }
                if !values.iter().all(suits) {
                    return Err(mismatch);
                }
            }
            FilterTest::Range { from, to } => {
                if from.is_none() && to.is_none() {
                    return Err(ModelError::Invalid("missing"));
                }
                if !from.iter().chain(to.iter()).all(suits) {
                    return Err(mismatch);
                }
                if matches!(field.value(), FieldValue::YesNo) {
                    return Err(mismatch);
                }
                let backwards = match (from, to) {
                    (Some(SpecValue::Day(a)), Some(SpecValue::Day(b))) => a > b,
                    (Some(SpecValue::Number(a)), Some(SpecValue::Number(b))) => {
                        a.compare(*b) == std::cmp::Ordering::Greater
                    }
                    (Some(SpecValue::Text(_)), _) | (_, Some(SpecValue::Text(_))) => true,
                    _ => false,
                };
                if backwards {
                    return Err(ModelError::Invalid("bad-choice"));
                }
            }
        }
        Ok(Self { field, test })
    }

    /// The field.
    #[must_use]
    pub const fn field(&self) -> &'static SpecField {
        self.field
    }

    /// The test.
    #[must_use]
    pub const fn test(&self) -> &FilterTest {
        &self.test
    }
}

/// One mark with its encodings and filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    mark: Mark,
    encodings: Vec<Encoding>,
    filters: Vec<FieldFilter>,
}

impl Layer {
    /// Checks the channels against the mark (design section 1): `x` and `y`
    /// for a line, bar, area or point (`size` also on a point); `theta` and
    /// `color` for an arc; `text` for a text (with `x`, `y`, `color`); exactly
    /// one of `x` and `y` for a rule. `tooltip` may repeat, other channels
    /// once.
    ///
    /// # Errors
    ///
    /// `Invalid("missing")`, `Invalid("bad-channel-for-mark")` or
    /// `TooBig("encodings" | "filters")`.
    pub fn new(
        mark: Mark,
        encodings: Vec<Encoding>,
        filters: Vec<FieldFilter>,
    ) -> Result<Self, ModelError> {
        if encodings.len() > MAX_ENCODINGS {
            return Err(ModelError::TooBig("encodings"));
        }
        if filters.len() > MAX_FILTERS {
            return Err(ModelError::TooBig("filters"));
        }
        let has = |channel: Channel| encodings.iter().any(|e| e.channel == channel);
        let allowed: &[Channel] = match mark.kind {
            MarkKind::Line | MarkKind::Bar | MarkKind::Area => {
                &[Channel::X, Channel::Y, Channel::Color, Channel::Tooltip]
            }
            MarkKind::Point => &[
                Channel::X,
                Channel::Y,
                Channel::Color,
                Channel::Size,
                Channel::Tooltip,
            ],
            MarkKind::Arc => &[Channel::Theta, Channel::Color, Channel::Tooltip],
            MarkKind::Text => &[Channel::Text, Channel::X, Channel::Y, Channel::Color],
            MarkKind::Rule => &[Channel::X, Channel::Y, Channel::Color],
        };
        for (index, encoding) in encodings.iter().enumerate() {
            let repeated = encoding.channel != Channel::Tooltip
                && encodings[..index]
                    .iter()
                    .any(|e| e.channel == encoding.channel);
            if !allowed.contains(&encoding.channel) || repeated {
                return Err(ModelError::Invalid("bad-channel-for-mark"));
            }
        }
        let needs: &[Channel] = match mark.kind {
            MarkKind::Line | MarkKind::Bar | MarkKind::Area | MarkKind::Point => {
                &[Channel::X, Channel::Y]
            }
            MarkKind::Arc => &[Channel::Theta, Channel::Color],
            MarkKind::Text => &[Channel::Text],
            MarkKind::Rule => &[],
        };
        if needs.iter().any(|channel| !has(*channel)) {
            return Err(ModelError::Invalid("missing"));
        }
        if mark.kind == MarkKind::Rule && has(Channel::X) == has(Channel::Y) {
            return Err(ModelError::Invalid(if has(Channel::X) {
                "bad-channel-for-mark"
            } else {
                "missing"
            }));
        }
        Ok(Self {
            mark,
            encodings,
            filters,
        })
    }

    /// The mark.
    #[must_use]
    pub const fn mark(&self) -> Mark {
        self.mark
    }

    /// The encodings.
    #[must_use]
    pub fn encodings(&self) -> &[Encoding] {
        &self.encodings
    }

    /// The layer's own filters.
    #[must_use]
    pub fn filters(&self) -> &[FieldFilter] {
        &self.filters
    }
}

/// One chart description, read into the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartSpec {
    dataset: &'static SpecDataset,
    title: Option<SpecText>,
    description: Option<SpecText>,
    size: Option<ChartSize>,
    palette: Vec<SpecColor>,
    filters: Vec<FieldFilter>,
    layers: Vec<Layer>,
}

impl ChartSpec {
    /// Checks the whole: 1 to 4 layers, at most 8 encodings and 8 filters
    /// together, at most 8 palette colors (words or `#rrggbb`), every field a field of `dataset`,
    /// an arc layer alone, and the other layers on one shared category axis
    /// (every layer with an `x` names the same field, type and time unit).
    ///
    /// # Errors
    ///
    /// `Invalid("missing" | "unknown-field" | "layers-differ")` or
    /// `TooBig("layers" | "encodings" | "filters" | "values")`.
    pub fn new(
        dataset: &'static SpecDataset,
        title: Option<SpecText>,
        description: Option<SpecText>,
        size: Option<ChartSize>,
        palette: Vec<SpecColor>,
        filters: Vec<FieldFilter>,
        layers: Vec<Layer>,
    ) -> Result<Self, ModelError> {
        if layers.is_empty() {
            return Err(ModelError::Invalid("missing"));
        }
        if layers.len() > MAX_LAYERS {
            return Err(ModelError::TooBig("layers"));
        }
        if palette.len() > MAX_PALETTE {
            return Err(ModelError::TooBig("values"));
        }
        let encodings: usize = layers.iter().map(|layer| layer.encodings.len()).sum();
        if encodings > MAX_ENCODINGS {
            return Err(ModelError::TooBig("encodings"));
        }
        let all_filters = || filters.iter().chain(layers.iter().flat_map(|l| &l.filters));
        if all_filters().count() > MAX_FILTERS {
            return Err(ModelError::TooBig("filters"));
        }
        let unknown = layers
            .iter()
            .flat_map(|layer| {
                layer.encodings.iter().flat_map(|e| {
                    let by = match e.sort {
                        Some(Sort::ByField(field)) => Some(field),
                        _ => None,
                    };
                    [Some(e.field), by]
                })
            })
            .flatten()
            .chain(all_filters().map(|filter| filter.field))
            .any(|field| !dataset.has(field));
        if unknown {
            return Err(ModelError::Invalid("unknown-field"));
        }
        if layers.len() > 1 && layers.iter().any(|l| l.mark.kind == MarkKind::Arc) {
            return Err(ModelError::Invalid("layers-differ"));
        }
        let mut axes = layers
            .iter()
            .filter_map(|layer| layer.encodings.iter().find(|e| e.channel == Channel::X))
            .map(|x| (x.field, x.kind, x.time_unit));
        if let Some(first) = axes.next()
            && axes.any(|other| other != first)
        {
            return Err(ModelError::Invalid("layers-differ"));
        }
        Ok(Self {
            dataset,
            title,
            description,
            size,
            palette,
            filters,
            layers,
        })
    }

    /// The dataset named in `data.name`.
    #[must_use]
    pub const fn dataset(&self) -> &'static SpecDataset {
        self.dataset
    }

    /// The title.
    #[must_use]
    pub const fn title(&self) -> Option<&SpecText> {
        self.title.as_ref()
    }

    /// The description.
    #[must_use]
    pub const fn description(&self) -> Option<&SpecText> {
        self.description.as_ref()
    }

    /// The size.
    #[must_use]
    pub const fn size(&self) -> Option<ChartSize> {
        self.size
    }

    /// The palette colors, empty for the default.
    #[must_use]
    pub fn palette(&self) -> &[SpecColor] {
        &self.palette
    }

    /// The top-level filters.
    #[must_use]
    pub fn filters(&self) -> &[FieldFilter] {
        &self.filters
    }

    /// The layers.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
}
