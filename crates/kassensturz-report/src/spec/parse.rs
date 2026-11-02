// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The parser of chart descriptions (M6e.1, D-505 to D-507): the one way in
//! for a pasted description and an opened file.
//!
//! Checks run in this order, and each runs before any work of the next one:
//! size, the raw scan (depth and repeated keys, [`super::limits`]), the
//! parse by `serde_json`, and a walk against the subset of
//! `docs/REPORT_DESIGNER.md` section 5. The walk has two halves. The first
//! goes through the text in its own order, checks every value and count as
//! it is reached, and lists every part the subset doesn't have. The second
//! resolves field names against the named dataset and builds the model,
//! whose constructors enforce the rules both halves share.
//!
//! Numbers never pass through floating point (D-506): a JSON number is read
//! only with `as_u64` and `as_i64`, and one with a fraction or an exponent is
//! refused with `fraction-number`; a decimal is written as a string.

use serde_json::{Map, Value};

use super::fields::{FieldValue, SpecDataset, SpecField, spec_dataset};
use super::limits::{self, MAX_SPEC_BYTES, NO_ORDER, Order, Refused, push_index, push_key};
use super::model::{
    Aggregate, Channel, ChartSize, ChartSpec, Encoding, FieldFilter, FieldType, FilterTest, Layer,
    MAX_ENCODINGS, MAX_FILTERS, MAX_LAYERS, MAX_ONE_OF, MAX_PALETTE, Mark, MarkKind, ModelError,
    PaletteName, Sort, SpecNumber, SpecText, SpecValue, TimeUnit, VL_SUBSET,
};
use super::unsupported::{self as why, Unsupported};
use crate::ReportError;
use kassensturz_core::Date;

/// The start every `$schema` must have (D-280). It is checked as text and
/// never fetched.
pub const SCHEMA_PREFIX: &str = "https://vega.github.io/schema/vega-lite/";

/// A chart description, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    chart: Option<ChartSpec>,
    unsupported: Vec<Unsupported>,
    /// Where something needed is missing, when `chart` is `None`.
    missing: Option<String>,
}

impl Read {
    /// The chart, or `None` when what is left after the unsupported parts
    /// can't be drawn. With an empty [`Read::unsupported`] list it is always
    /// there: a description that is fully supported and can't be drawn is
    /// refused by [`read`] instead.
    #[must_use]
    pub const fn chart(&self) -> Option<&ChartSpec> {
        self.chart.as_ref()
    }

    /// Every part that is not read, in the order of the text. Empty for a
    /// description that is fully supported.
    #[must_use]
    pub fn unsupported(&self) -> &[Unsupported] {
        &self.unsupported
    }

    /// Why nothing can be drawn: `report.spec-invalid` with `missing` and the
    /// path of what is missing (for example `data.name` when only `data.url`
    /// was given). `None` when there is a chart.
    #[must_use]
    pub fn why_not(&self) -> Option<ReportError> {
        self.missing.as_ref().map(|path| invalid("missing", path))
    }

    /// The chart and the list, taken apart.
    #[must_use]
    pub fn into_parts(self) -> (Option<ChartSpec>, Vec<Unsupported>) {
        (self.chart, self.unsupported)
    }
}

/// Reads a chart description.
///
/// # Errors
///
/// `report.spec-too-big` with the limit that is over (`bytes`, `depth`,
/// `layers`, `encodings`, `filters`, `values`, `label`), or
/// `report.spec-invalid` with a reason and the path of the value. Parts the
/// subset doesn't have are not errors; they are in [`Read::unsupported`].
pub fn read(text: &str) -> Result<Read, ReportError> {
    if text.len() > MAX_SPEC_BYTES {
        return Err(too_big("bytes"));
    }
    let order = limits::scan(text).map_err(|refused| match refused {
        Refused::TooDeep => too_big("depth"),
        Refused::DuplicateKey(path) => invalid("duplicate-key", &path),
        Refused::NotJson => invalid("not-json", ""),
    })?;
    let value: Value = serde_json::from_str(text).map_err(|_| invalid("not-json", ""))?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("not-an-object", ""))?;
    let mut walk = Walk::default();
    let raw = walk.spec(object, &order)?;
    let built = build(&raw, &mut walk);
    let mut notes = walk.notes;
    notes.sort_by_key(|(tick, _)| *tick);
    let mut unsupported: Vec<Unsupported> = notes.into_iter().map(|(_, entry)| entry).collect();
    unsupported.dedup();
    match built {
        Ok(chart) => Ok(Read {
            chart: Some(chart),
            unsupported,
            missing: None,
        }),
        Err(Stop::Missing(path)) if !unsupported.is_empty() => Ok(Read {
            chart: None,
            unsupported,
            missing: Some(path),
        }),
        Err(Stop::Missing(path)) => Err(invalid("missing", &path)),
        Err(Stop::Refused(error)) => Err(error),
    }
}

fn invalid(reason: &'static str, path: &str) -> ReportError {
    ReportError::SpecInvalid {
        reason,
        path: path.to_owned(),
    }
}

fn too_big(limit: &'static str) -> ReportError {
    ReportError::SpecTooBig { limit }
}

fn model(error: ModelError, path: &str) -> ReportError {
    match error {
        ModelError::Invalid(reason) => invalid(reason, path),
        ModelError::TooBig(limit) => too_big(limit),
    }
}

fn key(parent: &str, name: &str) -> String {
    let mut path = parent.to_owned();
    push_key(&mut path, name);
    path
}

fn index(parent: &str, at: usize) -> String {
    let mut path = parent.to_owned();
    push_index(&mut path, at);
    path
}

/// An object's entries in the order of the text, with the order of each
/// value. If the recorded order doesn't fit the object (it always does for
/// text that parsed), the map's own order is used.
fn entries<'a>(
    map: &'a Map<String, Value>,
    order: &'a Order,
) -> Vec<(&'a str, &'a Value, &'a Order)> {
    let keys = order.keys();
    if keys.len() == map.len() {
        let found: Vec<_> = keys
            .iter()
            .filter_map(|(name, inner)| {
                map.get_key_value(name)
                    .map(|(name, value)| (name.as_str(), value, inner))
            })
            .collect();
        if found.len() == map.len() {
            return found;
        }
    }
    map.iter()
        .map(|(name, value)| (name.as_str(), value, &NO_ORDER))
        .collect()
}

fn is_expression(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|map| map.contains_key("expr"))
}

/// A whole number from JSON, or `None` with a note when the value is not a
/// number of the subset at all.
enum Whole {
    Number(u64),
    Noted,
}

/// Why the second half stopped.
enum Stop {
    /// Something needed is missing, at this path. When parts were listed as
    /// unsupported this is not an error: the list explains it, and nothing
    /// is drawn.
    Missing(String),
    /// The description is refused.
    Refused(ReportError),
}

impl From<ReportError> for Stop {
    fn from(error: ReportError) -> Self {
        Self::Refused(error)
    }
}

/// The first half's view of one single-view part: the top level, or a layer.
#[derive(Debug, Default)]
struct RawView<'a> {
    /// `""` for the top level, `layer[n]` for a layer.
    path: String,
    mark: Option<RawMark>,
    /// The tick of the `mark` key, when there is one (supported or not).
    mark_tick: Option<usize>,
    encodings: Vec<RawEncoding<'a>>,
    filters: Vec<RawFilter<'a>>,
}

#[derive(Debug, Clone)]
struct RawMark {
    kind: MarkKind,
    tooltip: bool,
    point: bool,
    inner_radius: Option<u16>,
    path: String,
}

#[derive(Debug, Clone)]
struct RawEncoding<'a> {
    channel: Channel,
    path: String,
    tick: usize,
    /// The field's name and the path of `field`; `None` for a count without
    /// a field.
    field: Option<(&'a str, String)>,
    kind: FieldType,
    title: Option<SpecText>,
    aggregate: Option<Aggregate>,
    time_unit: Option<TimeUnit>,
    sort: Option<RawSort<'a>>,
}

#[derive(Debug, Clone)]
enum RawSort<'a> {
    Ascending,
    Descending,
    ByField(&'a str, String),
}

#[derive(Debug, Clone)]
struct RawFilter<'a> {
    path: String,
    field: &'a str,
    field_path: String,
    test: RawTest<'a>,
}

/// A filter's values as JSON, each with its path. What they are read as
/// depends on the field, which is known only in the second half.
#[derive(Debug, Clone)]
enum RawTest<'a> {
    Equal(&'a Value, String),
    OneOf(Vec<(&'a Value, String)>),
    Range(Option<(&'a Value, String)>, Option<(&'a Value, String)>),
}

#[derive(Debug, Default)]
struct RawSpec<'a> {
    dataset: Option<&'static SpecDataset>,
    title: Option<SpecText>,
    description: Option<SpecText>,
    width: Option<u16>,
    height: Option<u16>,
    palette: Vec<PaletteName>,
    /// The top level: its mark and encodings are the chart's only layer when
    /// there is no `layer`; its filters are the chart's own.
    top: RawView<'a>,
    layers: Option<Vec<RawView<'a>>>,
}

/// The first half: the walk through the text.
#[derive(Default)]
struct Walk {
    /// The unsupported parts, each with the tick it was found at.
    notes: Vec<(usize, Unsupported)>,
    tick: usize,
    encodings: usize,
    filters: usize,
}

impl Walk {
    fn stamp(&mut self) -> usize {
        self.tick += 1;
        self.tick
    }

    fn note(&mut self, path: String, reason: &'static str) {
        let tick = self.stamp();
        self.notes.push((tick, Unsupported::new(path, reason)));
    }

    /// A value of an allowed key that isn't one the subset reads: an
    /// expression, or another shape Vega-Lite has (an object, a list,
    /// `null`), is listed; a plain value of the wrong JSON type is refused.
    fn other(&mut self, value: &Value, path: String) -> Result<(), ReportError> {
        if is_expression(value) {
            self.note(path, why::EXPRESSION);
            Ok(())
        } else if value.is_object() || value.is_array() || value.is_null() {
            self.note(path, why::VALUE);
            Ok(())
        } else {
            Err(invalid("wrong-type", &path))
        }
    }

    fn spec<'a>(
        &mut self,
        map: &'a Map<String, Value>,
        order: &'a Order,
    ) -> Result<RawSpec<'a>, ReportError> {
        let mut spec = RawSpec::default();
        for (name, value, inner) in entries(map, order) {
            let path = key("", name);
            match name {
                "$schema" => {
                    if !value.as_str().is_some_and(|s| s.starts_with(SCHEMA_PREFIX)) {
                        return Err(invalid("bad-schema", &path));
                    }
                }
                "kassensturz" => self.ours(value, inner, &path)?,
                "data" => spec.dataset = self.data(value, inner, &path)?,
                "title" => spec.title = self.text(value, path)?,
                "description" => spec.description = self.text(value, path)?,
                "width" => spec.width = self.side(value, path)?,
                "height" => spec.height = self.side(value, path)?,
                "config" => spec.palette = self.config(value, inner, &path)?,
                "layer" => spec.layers = Some(self.layers(value, inner, &path)?),
                "datasets" => self.note(path, why::DATA_SOURCE),
                "facet" | "repeat" | "concat" | "hconcat" | "vconcat" | "params" | "selection"
                | "spec" | "resolve" => self.note(path, why::COMPOSITION),
                _ => {
                    if !self.view_key(&mut spec.top, name, value, inner, path)? {
                        let path = key("", name);
                        self.note(path, why::OPTION);
                    }
                }
            }
        }
        Ok(spec)
    }

    /// A key of a single view (`mark`, `encoding`, `transform`); `false` when
    /// it is none of them.
    fn view_key<'a>(
        &mut self,
        view: &mut RawView<'a>,
        name: &str,
        value: &'a Value,
        order: &'a Order,
        path: String,
    ) -> Result<bool, ReportError> {
        match name {
            "mark" => {
                view.mark_tick = Some(self.stamp());
                view.mark = self.mark(value, order, path)?;
            }
            "encoding" => view.encodings = self.encoding(value, order, &path)?,
            "transform" => view.filters = self.transform(value, order, &path)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Our own key, `"kassensturz": {"vl_subset": 1}` (D-280).
    fn ours(&mut self, value: &Value, order: &Order, path: &str) -> Result<(), ReportError> {
        let map = value
            .as_object()
            .ok_or_else(|| invalid("wrong-type", path))?;
        for (name, value, _) in entries(map, order) {
            let path = key(path, name);
            if name == "vl_subset" {
                if value.as_u64() != Some(u64::from(VL_SUBSET)) {
                    return Err(invalid("bad-choice", &path));
                }
            } else {
                self.note(path, why::OPTION);
            }
        }
        Ok(())
    }

    fn data(
        &mut self,
        value: &Value,
        order: &Order,
        path: &str,
    ) -> Result<Option<&'static SpecDataset>, ReportError> {
        let map = value
            .as_object()
            .ok_or_else(|| invalid("wrong-type", path))?;
        let mut dataset = None;
        for (name, value, _) in entries(map, order) {
            let path = key(path, name);
            match name {
                "name" => {
                    let id = value.as_str().ok_or_else(|| invalid("wrong-type", &path))?;
                    dataset =
                        Some(spec_dataset(id).ok_or_else(|| invalid("unknown-dataset", &path))?);
                }
                "url" | "values" | "sequence" | "format" => self.note(path, why::DATA_SOURCE),
                _ => self.note(path, why::OPTION),
            }
        }
        Ok(dataset)
    }

    fn text(&mut self, value: &Value, path: String) -> Result<Option<SpecText>, ReportError> {
        match value.as_str() {
            Some(text) => SpecText::new(text).map(Some).map_err(|e| model(e, &path)),
            None => self.other(value, path).map(|()| None),
        }
    }

    /// A whole number, or a note. A number with a fraction or an exponent is
    /// refused (D-506), and so is one below zero.
    fn whole(&mut self, value: &Value, path: String) -> Result<Whole, ReportError> {
        if let Some(number) = value.as_u64() {
            Ok(Whole::Number(number))
        } else if value.is_i64() {
            Err(invalid("bad-number", &path))
        } else if value.is_f64() {
            Err(invalid("fraction-number", &path))
        } else if value.is_string() {
            // `"container"` and the like.
            self.note(path, why::VALUE);
            Ok(Whole::Noted)
        } else {
            self.other(value, path).map(|()| Whole::Noted)
        }
    }

    /// `width` or `height`: a whole number from 100 to 2000.
    fn side(&mut self, value: &Value, path: String) -> Result<Option<u16>, ReportError> {
        match self.whole(value, path.clone())? {
            Whole::Number(number) => {
                let side = u16::try_from(number).map_err(|_| invalid("bad-number", &path))?;
                ChartSize::new(Some(side), None).map_err(|e| model(e, &path))?;
                Ok(Some(side))
            }
            Whole::Noted => Ok(None),
        }
    }

    /// `config`: only `range.category`, a list of palette words.
    fn config(
        &mut self,
        value: &Value,
        order: &Order,
        path: &str,
    ) -> Result<Vec<PaletteName>, ReportError> {
        let map = value
            .as_object()
            .ok_or_else(|| invalid("wrong-type", path))?;
        let mut palette = Vec::new();
        for (name, value, inner) in entries(map, order) {
            let path = key(path, name);
            if name != "range" {
                self.note(path, why::OPTION);
                continue;
            }
            let range = value
                .as_object()
                .ok_or_else(|| invalid("wrong-type", &path))?;
            for (name, value, _) in entries(range, inner) {
                let path = key(&path, name);
                if name != "category" {
                    self.note(path, why::OPTION);
                    continue;
                }
                let Some(colors) = value.as_array() else {
                    // A scheme object and the like.
                    self.other(value, path)?;
                    continue;
                };
                if colors.len() > MAX_PALETTE {
                    return Err(too_big("values"));
                }
                for (at, color) in colors.iter().enumerate() {
                    let path = index(&path, at);
                    match color.as_str() {
                        Some(word) => match PaletteName::from_word(word) {
                            Some(name) => palette.push(name),
                            // A color by its code, such as "#ff0000".
                            None => self.note(path, why::VALUE),
                        },
                        None => self.other(color, path)?,
                    }
                }
            }
        }
        Ok(palette)
    }

    fn layers<'a>(
        &mut self,
        value: &'a Value,
        order: &'a Order,
        path: &str,
    ) -> Result<Vec<RawView<'a>>, ReportError> {
        let list = value
            .as_array()
            .ok_or_else(|| invalid("wrong-type", path))?;
        if list.len() > MAX_LAYERS {
            return Err(too_big("layers"));
        }
        if list.is_empty() {
            return Err(invalid("missing", path));
        }
        let mut views = Vec::with_capacity(list.len());
        for (at, item) in list.iter().enumerate() {
            let path = index(path, at);
            let map = item
                .as_object()
                .ok_or_else(|| invalid("wrong-type", &path))?;
            let mut view = RawView {
                path: path.clone(),
                ..RawView::default()
            };
            for (name, value, inner) in entries(map, order.item(at)) {
                let inner_path = key(&path, name);
                if name == "layer" {
                    self.note(inner_path, why::COMPOSITION);
                } else if !self.view_key(&mut view, name, value, inner, inner_path)? {
                    // A layer's own data, title, size and the like.
                    self.note(key(&path, name), why::OPTION);
                }
            }
            views.push(view);
        }
        Ok(views)
    }

    fn mark(
        &mut self,
        value: &Value,
        order: &Order,
        path: String,
    ) -> Result<Option<RawMark>, ReportError> {
        if let Some(word) = value.as_str() {
            return Ok(self.mark_kind(word, path.clone()).map(|kind| RawMark {
                kind,
                tooltip: false,
                point: false,
                inner_radius: None,
                path,
            }));
        }
        let map = value
            .as_object()
            .ok_or_else(|| invalid("wrong-type", &path))?;
        let mut kind = None;
        let mut has_type = false;
        let (mut tooltip, mut point, mut inner_radius) = (false, false, None);
        for (name, value, _) in entries(map, order) {
            let at = key(&path, name);
            match name {
                "type" => {
                    has_type = true;
                    let word = value.as_str().ok_or_else(|| invalid("wrong-type", &at))?;
                    kind = self.mark_kind(word, at);
                }
                "tooltip" => match value.as_bool() {
                    Some(on) => tooltip = on,
                    None => self.other(value, at)?,
                },
                "point" => match value.as_bool() {
                    Some(on) => point = on,
                    None => self.other(value, at)?,
                },
                "innerRadius" => {
                    if let Whole::Number(number) = self.whole(value, at.clone())? {
                        let radius =
                            u16::try_from(number).map_err(|_| invalid("bad-number", &at))?;
                        // 0 is Vega-Lite's default, a pie.
                        inner_radius = (radius > 0).then_some(radius);
                    }
                }
                _ => self.note(at, why::OPTION),
            }
        }
        if !has_type {
            return Err(invalid("missing", &key(&path, "type")));
        }
        Ok(kind.map(|kind| RawMark {
            kind,
            tooltip,
            point,
            inner_radius,
            path,
        }))
    }

    fn mark_kind(&mut self, word: &str, path: String) -> Option<MarkKind> {
        let kind = match word {
            "line" => MarkKind::Line,
            "bar" => MarkKind::Bar,
            "area" => MarkKind::Area,
            "arc" => MarkKind::Arc,
            "point" => MarkKind::Point,
            "text" => MarkKind::Text,
            "rule" => MarkKind::Rule,
            _ => {
                self.note(path, why::MARK);
                return None;
            }
        };
        Some(kind)
    }

    fn encoding<'a>(
        &mut self,
        value: &'a Value,
        order: &'a Order,
        path: &str,
    ) -> Result<Vec<RawEncoding<'a>>, ReportError> {
        let map = value
            .as_object()
            .ok_or_else(|| invalid("wrong-type", path))?;
        let mut encodings = Vec::new();
        for (name, value, inner) in entries(map, order) {
            let path = key(path, name);
            let channel = match name {
                "x" => Channel::X,
                "y" => Channel::Y,
                "color" => Channel::Color,
                "size" => Channel::Size,
                "theta" => Channel::Theta,
                "text" => Channel::Text,
                "tooltip" => Channel::Tooltip,
                _ => {
                    self.note(path, why::CHANNEL);
                    continue;
                }
            };
            if let (Channel::Tooltip, Some(list)) = (channel, value.as_array()) {
                if list.len() > MAX_ENCODINGS {
                    return Err(too_big("encodings"));
                }
                for (at, item) in list.iter().enumerate() {
                    let path = index(&path, at);
                    let map = item
                        .as_object()
                        .ok_or_else(|| invalid("wrong-type", &path))?;
                    encodings.extend(self.field_def(channel, map, inner.item(at), path)?);
                }
            } else if let Some(map) = value.as_object() {
                encodings.extend(self.field_def(channel, map, inner, path)?);
            } else {
                self.other(value, path)?;
            }
        }
        Ok(encodings)
    }

    /// One field on one channel. `None` when a part that decides what the
    /// encoding shows (its field, type, aggregate, time unit or binning) is
    /// not supported: drawing it without that part would show other figures,
    /// so the encoding is left out whole, and the list says why.
    fn field_def<'a>(
        &mut self,
        channel: Channel,
        map: &'a Map<String, Value>,
        order: &'a Order,
        path: String,
    ) -> Result<Option<RawEncoding<'a>>, ReportError> {
        self.encodings += 1;
        if self.encodings > MAX_ENCODINGS {
            return Err(too_big("encodings"));
        }
        let tick = self.stamp();
        let before = self.notes.len();
        let mut broken = false;
        let mut field = None;
        let mut kind = None;
        let mut title = None;
        let mut aggregate = None;
        let mut time_unit = None;
        let mut sort = None;
        for (name, value, _) in entries(map, order) {
            let at = key(&path, name);
            match name {
                "field" => match value.as_str() {
                    Some(name) => field = Some((name, at)),
                    None => {
                        broken = true;
                        if value.as_object().is_some_and(|m| m.contains_key("repeat")) {
                            self.note(at, why::COMPOSITION);
                        } else {
                            self.other(value, at)?;
                        }
                    }
                },
                "type" => {
                    kind = self.choice(value, at, TYPES)?;
                    broken |= kind.is_none();
                }
                "title" => title = self.text(value, at)?,
                "aggregate" => {
                    aggregate = self.choice(value, at, AGGREGATES)?;
                    broken |= aggregate.is_none();
                }
                "timeUnit" => {
                    time_unit = self.choice(value, at, TIME_UNITS)?;
                    broken |= time_unit.is_none();
                }
                "sort" => sort = self.sort(value, at)?,
                "bin" => {
                    broken = true;
                    self.note(at, why::OPTION);
                }
                "condition" => self.note(at, why::EXPRESSION),
                "value" if is_expression(value) => self.note(at, why::EXPRESSION),
                _ => self.note(at, why::OPTION),
            }
        }
        if broken {
            return Ok(None);
        }
        // Vega-Lite reads an aggregate as a quantity and a time unit as a
        // day when `type` is left out; nothing else is guessed.
        let kind = kind.or(match (aggregate, time_unit) {
            (Some(_), _) => Some(FieldType::Quantitative),
            (None, Some(_)) => Some(FieldType::Temporal),
            (None, None) => None,
        });
        let complete = kind.is_some() && (field.is_some() || aggregate == Some(Aggregate::Count));
        if !complete {
            // A `value` or `datum` without a field was listed; anything else
            // is missing its field or type.
            if self.notes.len() > before {
                return Ok(None);
            }
            let missing = if kind.is_none() { "type" } else { "field" };
            return Err(invalid("missing", &key(&path, missing)));
        }
        Ok(kind.map(|kind| RawEncoding {
            channel,
            path,
            tick,
            field,
            kind,
            title,
            aggregate,
            time_unit,
            sort,
        }))
    }

    /// One of `words`, or a note: another word is a value outside the subset.
    fn choice<T: Copy>(
        &mut self,
        value: &Value,
        path: String,
        words: &[(&str, T)],
    ) -> Result<Option<T>, ReportError> {
        let Some(word) = value.as_str() else {
            return self.other(value, path).map(|()| None);
        };
        match words.iter().find(|(own, _)| *own == word) {
            Some((_, choice)) => Ok(Some(*choice)),
            None => {
                self.note(path, why::VALUE);
                Ok(None)
            }
        }
    }

    fn sort<'a>(
        &mut self,
        value: &'a Value,
        path: String,
    ) -> Result<Option<RawSort<'a>>, ReportError> {
        let Some(word) = value.as_str() else {
            // `null`, a list of values, or a sort object.
            self.other(value, path)?;
            return Ok(None);
        };
        Ok(match word {
            "ascending" => Some(RawSort::Ascending),
            "descending" => Some(RawSort::Descending),
            _ if is_channel_sort(word) => {
                self.note(path, why::VALUE);
                None
            }
            _ => Some(RawSort::ByField(word, path)),
        })
    }

    fn transform<'a>(
        &mut self,
        value: &'a Value,
        order: &'a Order,
        path: &str,
    ) -> Result<Vec<RawFilter<'a>>, ReportError> {
        let list = value
            .as_array()
            .ok_or_else(|| invalid("wrong-type", path))?;
        let mut filters = Vec::new();
        for (at, item) in list.iter().enumerate() {
            let path = index(path, at);
            let map = item
                .as_object()
                .ok_or_else(|| invalid("wrong-type", &path))?;
            let fields = entries(map, order.item(at));
            if map.contains_key("filter") {
                for (name, value, inner) in fields {
                    let at = key(&path, name);
                    if name != "filter" {
                        self.note(at, why::OPTION);
                    } else if value.is_string() {
                        self.note(at, why::EXPRESSION);
                    } else if let Some(map) = value.as_object() {
                        filters.extend(self.filter(map, inner, at)?);
                    } else {
                        return Err(invalid("wrong-type", &at));
                    }
                }
                continue;
            }
            // One entry for the whole transform, at the key that names it.
            let named = fields
                .iter()
                .find(|(name, _, _)| TRANSFORMS.contains(name))
                .or_else(|| fields.first());
            match named {
                Some((name, _, _)) => {
                    let reason = if *name == "calculate" {
                        why::EXPRESSION
                    } else {
                        why::TRANSFORM
                    };
                    self.note(key(&path, name), reason);
                }
                None => return Err(invalid("missing", &path)),
            }
        }
        Ok(filters)
    }

    /// A filter object. `None` when a part of it is not supported: a filter
    /// without that part would let through rows it should not, so it is left
    /// out whole, and the list says why.
    fn filter<'a>(
        &mut self,
        map: &'a Map<String, Value>,
        order: &'a Order,
        path: String,
    ) -> Result<Option<RawFilter<'a>>, ReportError> {
        self.filters += 1;
        if self.filters > MAX_FILTERS {
            return Err(too_big("filters"));
        }
        let before = self.notes.len();
        let mut field = None;
        let mut tests = Vec::new();
        for (name, value, _) in entries(map, order) {
            let at = key(&path, name);
            match name {
                "field" => {
                    let name = value.as_str().ok_or_else(|| invalid("wrong-type", &at))?;
                    field = Some((name, at));
                }
                "equal" => {
                    if self.filter_value(value, &at, false)? {
                        tests.push(RawTest::Equal(value, at));
                    }
                }
                "oneOf" => {
                    let list = value.as_array().ok_or_else(|| invalid("wrong-type", &at))?;
                    if list.len() > MAX_ONE_OF {
                        return Err(too_big("values"));
                    }
                    let mut values = Vec::with_capacity(list.len());
                    let mut fine = true;
                    for (index_at, item) in list.iter().enumerate() {
                        let item_path = index(&at, index_at);
                        if self.filter_value(item, &item_path, false)? {
                            values.push((item, item_path));
                        } else {
                            fine = false;
                        }
                    }
                    if fine {
                        tests.push(RawTest::OneOf(values));
                    }
                }
                "range" => {
                    let list = value.as_array().ok_or_else(|| invalid("wrong-type", &at))?;
                    if list.len() != 2 {
                        return Err(invalid("wrong-type", &at));
                    }
                    let mut ends = [None, None];
                    let mut fine = true;
                    for (end, item) in list.iter().enumerate() {
                        let item_path = index(&at, end);
                        if self.filter_value(item, &item_path, true)? {
                            ends[end] = (!item.is_null()).then_some((item, item_path));
                        } else {
                            fine = false;
                        }
                    }
                    if fine {
                        let [from, to] = ends;
                        tests.push(RawTest::Range(from, to));
                    }
                }
                "param" | "selection" => self.note(at, why::COMPOSITION),
                _ => self.note(at, why::OPTION),
            }
        }
        if self.notes.len() > before {
            return Ok(None);
        }
        let Some((field, field_path)) = field else {
            return Err(invalid("missing", &key(&path, "field")));
        };
        if tests.len() > 1 {
            return Err(invalid("bad-choice", &path));
        }
        let Some(test) = tests.pop() else {
            return Err(invalid("missing", &path));
        };
        Ok(Some(RawFilter {
            path,
            field,
            field_path,
            test,
        }))
    }

    /// Whether a filter's value is one the second half reads (a string, a
    /// number, `true` or `false`, and `null` for an open end of a range);
    /// anything else is listed.
    fn filter_value(
        &mut self,
        value: &Value,
        path: &str,
        open_end: bool,
    ) -> Result<bool, ReportError> {
        let plain = value.is_string()
            || value.is_number()
            || value.is_boolean()
            || (open_end && value.is_null());
        if plain {
            return Ok(true);
        }
        // A date-time object, an expression, a list.
        self.other(value, path.to_owned())?;
        Ok(false)
    }
}

const TYPES: &[(&str, FieldType)] = &[
    ("nominal", FieldType::Nominal),
    ("ordinal", FieldType::Ordinal),
    ("quantitative", FieldType::Quantitative),
    ("temporal", FieldType::Temporal),
];

const AGGREGATES: &[(&str, Aggregate)] = &[
    ("sum", Aggregate::Sum),
    ("mean", Aggregate::Mean),
    ("min", Aggregate::Min),
    ("max", Aggregate::Max),
    ("count", Aggregate::Count),
];

const TIME_UNITS: &[(&str, TimeUnit)] = &[
    ("year", TimeUnit::Year),
    ("yearmonth", TimeUnit::YearMonth),
    ("yearmonthdate", TimeUnit::YearMonthDate),
];

/// The transforms of Vega-Lite other than `filter`, so a transform is listed
/// at the key that names it rather than at a companion key such as `as`.
const TRANSFORMS: &[&str] = &[
    "calculate",
    "aggregate",
    "bin",
    "timeUnit",
    "lookup",
    "window",
    "joinaggregate",
    "fold",
    "pivot",
    "impute",
    "density",
    "loess",
    "regression",
    "quantile",
    "sample",
    "stack",
    "flatten",
    "extent",
];

/// Vega-Lite sorts by another channel with its name, `-` first for
/// descending (`"-y"`). The subset does not have that.
fn is_channel_sort(word: &str) -> bool {
    let channel = word.strip_prefix('-').unwrap_or(word);
    matches!(
        channel,
        "x" | "y"
            | "color"
            | "fill"
            | "stroke"
            | "strokeWidth"
            | "size"
            | "shape"
            | "fillOpacity"
            | "strokeOpacity"
            | "opacity"
            | "text"
            | "theta"
            | "radius"
    )
}

/// The channels each mark takes, as the model's `Layer::new` checks them;
/// the parser lists a channel the mark doesn't take instead of refusing it
/// (design section 1). A test keeps the two the same.
pub(super) fn takes(kind: MarkKind, channel: Channel) -> bool {
    use Channel::{Color, Size, Text, Theta, Tooltip, X, Y};
    match kind {
        MarkKind::Line | MarkKind::Bar | MarkKind::Area => {
            matches!(channel, X | Y | Color | Tooltip)
        }
        MarkKind::Point => matches!(channel, X | Y | Color | Size | Tooltip),
        MarkKind::Arc => matches!(channel, Theta | Color | Tooltip),
        MarkKind::Text => matches!(channel, Text | X | Y | Color),
        MarkKind::Rule => matches!(channel, X | Y | Color),
    }
}

/// The second half: names resolved against the dataset, the model built.
fn build(raw: &RawSpec<'_>, walk: &mut Walk) -> Result<ChartSpec, Stop> {
    let mut views: Vec<RawView<'_>> = Vec::new();
    let top_filters = &raw.top.filters;
    match &raw.layers {
        None => {
            views.push(RawView {
                path: String::new(),
                mark: raw.top.mark.as_ref().cloned(),
                mark_tick: raw.top.mark_tick,
                encodings: raw.top.encodings.clone(),
                filters: Vec::new(),
            });
        }
        Some(layers) => {
            if let Some(tick) = raw.top.mark_tick {
                // A layered description has its marks in the layers.
                walk.notes
                    .push((tick, Unsupported::new("mark".into(), why::OPTION)));
            }
            for layer in layers {
                // Encodings of the top level are shared by every layer that
                // doesn't set the same channel itself, as in Vega-Lite.
                let mut encodings = layer.encodings.clone();
                for shared in &raw.top.encodings {
                    if !layer.encodings.iter().any(|e| e.channel == shared.channel) {
                        encodings.push(shared.clone());
                    }
                }
                views.push(RawView {
                    path: layer.path.clone(),
                    mark: layer.mark.as_ref().cloned(),
                    mark_tick: layer.mark_tick,
                    encodings,
                    filters: layer.filters.clone(),
                });
            }
        }
    }
    let dataset = raw
        .dataset
        .ok_or_else(|| Stop::Missing("data.name".into()))?;
    let mut layers = Vec::with_capacity(views.len());
    for view in views {
        layers.push(layer(view, dataset, walk)?);
    }
    let filters = top_filters
        .iter()
        .map(|filter| field_filter(filter, dataset))
        .collect::<Result<Vec<_>, _>>()?;
    let size = if raw.width.is_some() || raw.height.is_some() {
        Some(ChartSize::new(raw.width, raw.height).map_err(|e| model(e, "width"))?)
    } else {
        None
    };
    let path = if raw.layers.is_some() { "layer" } else { "" };
    Ok(ChartSpec::new(
        dataset,
        raw.title.clone(),
        raw.description.clone(),
        size,
        raw.palette.clone(),
        filters,
        layers,
    )
    .map_err(|e| model(e, path))?)
}

fn layer(view: RawView<'_>, dataset: &'static SpecDataset, walk: &mut Walk) -> Result<Layer, Stop> {
    let RawView {
        path,
        mark,
        mut encodings,
        filters,
        ..
    } = view;
    let encoding_path = key(&path, "encoding");
    let Some(mark) = mark else {
        return Err(Stop::Missing(key(&path, "mark")));
    };
    // A quantitative `x` across a nominal `y`: a horizontal chart.
    let x_quantity = encodings
        .iter()
        .find(|e| e.channel == Channel::X)
        .filter(|x| x.kind == FieldType::Quantitative)
        .map(|x| x.tick);
    let y_quantity = encodings
        .iter()
        .find(|e| e.channel == Channel::Y)
        .map(|y| y.kind == FieldType::Quantitative);
    if let Some(tick) = x_quantity
        && y_quantity != Some(true)
        && takes(mark.kind, Channel::X)
    {
        walk.notes.push((
            tick,
            Unsupported::new(encoding_path.clone(), why::HORIZONTAL),
        ));
        encodings.retain(|e| !matches!(e.channel, Channel::X | Channel::Y));
    }
    encodings.retain(|e| {
        let taken = takes(mark.kind, e.channel);
        if !taken {
            walk.notes
                .push((e.tick, Unsupported::new(e.path.clone(), why::CHANNEL)));
        }
        taken
    });
    let needs: &[Channel] = match mark.kind {
        MarkKind::Line | MarkKind::Bar | MarkKind::Area | MarkKind::Point => {
            &[Channel::X, Channel::Y]
        }
        MarkKind::Arc => &[Channel::Theta, Channel::Color],
        MarkKind::Text => &[Channel::Text],
        MarkKind::Rule => &[],
    };
    let has = |channel: Channel| encodings.iter().any(|e| e.channel == channel);
    if let Some(missing) = needs.iter().find(|channel| !has(**channel)) {
        return Err(Stop::Missing(key(&encoding_path, channel_word(*missing))));
    }
    if mark.kind == MarkKind::Rule && !has(Channel::X) && !has(Channel::Y) {
        return Err(Stop::Missing(key(&encoding_path, "y")));
    }
    let built_mark = Mark::new(mark.kind, mark.tooltip, mark.point, mark.inner_radius)
        .map_err(|e| model(e, &mark.path))?;
    let mut built = Vec::with_capacity(encodings.len());
    for encoding in &encodings {
        built.push(encoding_of(encoding, dataset)?);
    }
    let filters = filters
        .iter()
        .map(|filter| field_filter(filter, dataset))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Layer::new(built_mark, built, filters).map_err(|e| model(e, &encoding_path))?)
}

fn channel_word(channel: Channel) -> &'static str {
    match channel {
        Channel::X => "x",
        Channel::Y => "y",
        Channel::Color => "color",
        Channel::Size => "size",
        Channel::Theta => "theta",
        Channel::Text => "text",
        Channel::Tooltip => "tooltip",
    }
}

fn field_of(
    dataset: &'static SpecDataset,
    name: &str,
    path: &str,
) -> Result<&'static SpecField, ReportError> {
    dataset
        .field(name)
        .ok_or_else(|| invalid("unknown-field", path))
}

fn encoding_of(raw: &RawEncoding<'_>, dataset: &'static SpecDataset) -> Result<Encoding, Stop> {
    let field = match &raw.field {
        Some((name, path)) => field_of(dataset, name, path)?,
        // A count without a field counts rows, which is the same for every
        // field; the dataset's first one stands in (D-513).
        None => dataset
            .fields()
            .first()
            .ok_or_else(|| invalid("unknown-field", &raw.path))?,
    };
    let sort = match &raw.sort {
        None => None,
        Some(RawSort::Ascending) => Some(Sort::Ascending),
        Some(RawSort::Descending) => Some(Sort::Descending),
        Some(RawSort::ByField(name, path)) => Some(Sort::ByField(field_of(dataset, name, path)?)),
    };
    Ok(Encoding::new(
        raw.channel,
        field,
        raw.kind,
        raw.title.clone(),
        raw.aggregate,
        raw.time_unit,
        sort,
    )
    .map_err(|e| model(e, &raw.path))?)
}

fn field_filter(raw: &RawFilter<'_>, dataset: &'static SpecDataset) -> Result<FieldFilter, Stop> {
    let field = field_of(dataset, raw.field, &raw.field_path)?;
    let one = |(value, path): &(&Value, String)| value_of(field, value, path);
    let test = match &raw.test {
        RawTest::Equal(value, path) => FilterTest::Equal(value_of(field, value, path)?),
        RawTest::OneOf(values) => {
            FilterTest::OneOf(values.iter().map(one).collect::<Result<_, _>>()?)
        }
        RawTest::Range(from, to) => FilterTest::Range {
            from: from.as_ref().map(one).transpose()?,
            to: to.as_ref().map(one).transpose()?,
        },
    };
    Ok(FieldFilter::new(field, test).map_err(|e| model(e, &raw.path))?)
}

/// A filter's value, read as the field's kind. A number is read from its
/// digits only (D-506).
fn value_of(field: &SpecField, value: &Value, path: &str) -> Result<SpecValue, ReportError> {
    let mismatch = || invalid("type-mismatch", path);
    match field.value() {
        FieldValue::Text => {
            let text = value.as_str().ok_or_else(mismatch)?;
            Ok(SpecValue::Text(
                SpecText::new(text).map_err(|e| model(e, path))?,
            ))
        }
        FieldValue::Day => {
            let text = value.as_str().ok_or_else(mismatch)?;
            Ok(SpecValue::Day(
                day(text).ok_or_else(|| invalid("bad-date", path))?,
            ))
        }
        FieldValue::Amount | FieldValue::Percent => {
            if let Some(text) = value.as_str() {
                return Ok(SpecValue::Number(
                    SpecNumber::parse(text).map_err(|e| model(e, path))?,
                ));
            }
            if let Some(whole) = value.as_i64() {
                Ok(SpecValue::Number(SpecNumber::whole(whole)))
            } else if value.is_u64() {
                Err(invalid("bad-number", path))
            } else if value.is_f64() {
                Err(invalid("fraction-number", path))
            } else {
                Err(mismatch())
            }
        }
        FieldValue::YesNo => Ok(SpecValue::YesNo(value.as_bool().ok_or_else(mismatch)?)),
    }
}

/// A day written `YYYY-MM-DD` (ISO 8601), nothing else.
fn day(text: &str) -> Option<Date> {
    let shaped = text.len() == 10
        && text.char_indices().all(|(at, c)| match at {
            4 | 7 => c == '-',
            _ => c.is_ascii_digit(),
        });
    if shaped {
        text.parse::<Date>().ok()
    } else {
        None
    }
}
