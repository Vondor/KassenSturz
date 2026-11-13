// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rows a prepared spending table shows, for the answer of
//! `propose_view` (US6.13b, #789, D-761).
//!
//! A model that prepares a table can't see it, so before this it could
//! only say what it asked for, not what the table shows. Now the answer
//! has the rows: KassenSturz works them out here with the same
//! `kassensturz-report` functions the table uses (spending by category,
//! its grouping, groups, categories, hidden categories, average and
//! comparison) and writes them as exact decimal text. With a comparison,
//! each row says which way it moved and the colour the table gives it
//! (D-760), so the model can say what the colours mean. Every figure goes
//! into the answer's figures, so the chat's number check passes it
//! (D-052).

use kassensturz_core::ai_switch::{Permissions, Row};
use kassensturz_core::{Date, GroupMember, Report};
use kassensturz_report::{
    Asked, Average, CHANGE_STEPS, CategoryRow, Change, Component, ComponentType, Filters, Level,
    NEUTRAL_WORDS, Overlap, Per, ReportError, SPENDING_BY_CATEGORY, Setting, Shown, Source,
    spending_by_category_shown, spending_compared,
};
use kassensturz_store::Book;
use serde_json::{Map, Value, json};

use super::view_figure::{Failure, dates, filter_of, period_of};
use super::{Proposal, ProposedChange};
use crate::error::ToolError;
use crate::writer::{Writer, decimal_text};

/// The most rows of one currency the answer lists; the rest are counted
/// in `rows_left_out`, and the total still counts them.
pub const MAX_TABLE_ROWS: usize = 40;

/// What the answer says about the rows, so the model relays them and never
/// works out a figure of its own (AGENTS.md §8).
const TABLE_NOTE: &str = "computed_rows is what the prepared table shows, worked out by \
    KassenSturz for its period (from, to), one entry per currency: each row's name and value, \
    then the total. With a compare, previous is the row's figure in the comparison, change is \
    value less previous and change_percent that change in percent of previous (missing when \
    previous is zero). direction is more, less or same spending than the comparison; colour is \
    what the table shows: green for less, red for more, none for no change or a change smaller \
    than the chosen neutral percent. Tell the user these figures as they are and never work out \
    one of your own. If the user changes the table before saving it, the saved table shows the \
    changed one.";

/// Puts the rows of the one spending table `proposal` prepares into its
/// answer: `computed_rows`, one entry per currency, and
/// `computed_rows_note`; or `not_computed` with the error code when the
/// rows can't be worked out (two chosen groups that share a category).
/// Tables of other data, and tables with columns of their own, get
/// nothing here: the read functions give their figures.
///
/// The rows are figures of the book, so they reach the model only when the
/// permission grid lets it read the same figures with `category_totals`
/// (the reading of the categories and of the transactions, D-252);
/// otherwise `not_shown` names `computed_rows`.
pub(super) fn add_table(
    book: &Book,
    proposal: &mut Proposal,
    allowed: Permissions,
    today: Date,
) -> Result<(), ToolError> {
    let (component, period) = match proposal.changes.changes() {
        [ProposedChange::NewView(new)] => (new.component(), new.period.as_str()),
        [ProposedChange::EditView(edit)] => (edit.component(), edit.period.as_str()),
        _ => return Ok(()),
    };
    if component.kind() != ComponentType::TableRows
        || component.dataset().id != SPENDING_BY_CATEGORY
        || matches!(component.get("columns"), Some(Setting::Columns(columns)) if !columns.is_empty())
    {
        return Ok(());
    }
    if !(allowed.reads(Row::Categories) && allowed.reads(Row::Transactions)) {
        if let Value::Object(value) = &mut proposal.output.value {
            value.insert("not_shown".into(), json!(["computed_rows"]));
        }
        return Ok(());
    }
    let mut writer = Writer::new(book);
    let answer = match rows(book, component, period, today, &mut writer) {
        Ok(entries) => vec![
            ("computed_rows", Value::Array(entries)),
            ("computed_rows_note", json!(TABLE_NOTE)),
        ],
        Err(Failure::Tool(error)) => return Err(error),
        Err(Failure::Report(ReportError::Store(error))) => return Err(ToolError::Store(error)),
        Err(Failure::Report(error)) => vec![("not_computed", json!(error.report().code))],
    };
    if let Value::Object(value) = &mut proposal.output.value {
        for (key, entry) in answer {
            value.insert(key.into(), entry);
        }
    }
    proposal.output.figures.extend(&writer.figures);
    Ok(())
}

/// The ids a list setting of `component` names.
fn ids(component: &Component, key: &str) -> Vec<String> {
    match component.get(key) {
        Some(Setting::Ids(ids)) => ids.clone(),
        _ => Vec::new(),
    }
}

fn word<'a>(component: &'a Component, key: &str) -> &'a str {
    match component.get(key) {
        Some(Setting::Word(word)) => word.as_str(),
        _ => "",
    }
}

/// The entries of `computed_rows` for `component` over the period `word`,
/// counted as the table counts them.
fn rows(
    book: &Book,
    component: &Component,
    period_word: &str,
    today: Date,
    writer: &mut Writer<'_>,
) -> Result<Vec<Value>, Failure> {
    let period = period_of(book, period_word, today)?;
    let asked = Asked::new(component, None, &Filters::default());
    let compare = asked.as_ref().and_then(|asked| asked.compare);
    let span = match compare {
        Some(compare) => compare.read_span(period)?,
        None => period,
    };
    let source = Source::read(book, span)?;
    let Some(filter) = filter_of(asked.as_ref(), &source)? else {
        return Ok(Vec::new());
    };
    let grouping = word(component, "grouping");
    let level = if grouping == "top_level_category" {
        Level::TopLevel
    } else {
        Level::Each
    };
    // Groups are rows only for the grouping that shows them (as the table).
    let groups = if grouping == "groups" {
        ids(component, "groups")
    } else {
        Vec::new()
    };
    let shown = Shown {
        groups: groups
            .iter()
            .filter_map(|id| match source.member_by_id(id) {
                Some(GroupMember::Group(id)) => Some(id),
                _ => None,
            })
            .collect(),
        only: ids(component, "categories")
            .iter()
            .filter_map(|id| source.member_by_id(id))
            .collect(),
        show_hidden: matches!(component.get("show_hidden"), Some(Setting::Flag(true))),
        // A table is not parts of a whole: a category may sit in two groups.
        overlap: Overlap::Allowed,
    };
    let rounding = book.rounding_mode().map_err(ReportError::Store)?;
    let per = asked.as_ref().and_then(|asked| asked.average);
    let average = per
        .map(|per| Average::new(per, period, today, rounding))
        .transpose()?;
    let of = |money, average: Option<&Average>| average.map_or(money, |average| average.of(money));
    // How many steps `neutral` asks a change to reach before it is coloured.
    let neutral_word = word(component, "neutral");
    let neutral = NEUTRAL_WORDS
        .iter()
        .position(|word| *word == neutral_word)
        .unwrap_or(0);
    let mut entries = Vec::new();
    if let Some(compare) = compare {
        let earlier = compare.divisor(period, per, today, rounding)?;
        for data in spending_compared(&source, period, &filter, level, &shown, compare)? {
            let mut entry = head(data.now.currency.as_str(), period, per);
            entry.insert("compare".into(), json!(compare.word()));
            let mut rows = Vec::new();
            for compared in data.rows.iter().take(MAX_TABLE_ROWS) {
                let change = Change::new(
                    earlier.of(compared.earlier),
                    of(compared.row.total, average.as_ref()),
                )?;
                let mut row = name(&compared.row, writer);
                put_change(&mut row, &change, neutral, writer)?;
                rows.push(Value::Object(row));
            }
            entry.insert("rows".into(), Value::Array(rows));
            left_out(&mut entry, data.rows.len(), writer);
            let total = Change::new(
                earlier.of(data.earlier_total),
                of(data.now.total, average.as_ref()),
            )?;
            let mut row = Map::new();
            put_change(&mut row, &total, neutral, writer)?;
            entry.insert("total".into(), Value::Object(row));
            entries.push(Value::Object(entry));
        }
    } else {
        for data in spending_by_category_shown(&source, period, &filter, level, &shown)? {
            let mut entry = head(data.currency.as_str(), period, per);
            let mut rows = Vec::new();
            for category in data.rows.iter().take(MAX_TABLE_ROWS) {
                let mut row = name(category, writer);
                let value = writer
                    .amount(of(category.total, average.as_ref()))
                    .map_err(ToolError::Store)?;
                row.insert("value".into(), value);
                rows.push(Value::Object(row));
            }
            entry.insert("rows".into(), Value::Array(rows));
            left_out(&mut entry, data.rows.len(), writer);
            let total = writer
                .amount(of(data.total, average.as_ref()))
                .map_err(ToolError::Store)?;
            entry.insert("total".into(), json!({ "value": total }));
            entries.push(Value::Object(entry));
        }
    }
    Ok(entries)
}

/// The start of a currency's entry: its currency, period and, for an
/// average, what it is an average per.
fn head(
    currency: &str,
    period: kassensturz_report::Period,
    per: Option<Per>,
) -> Map<String, Value> {
    let mut entry = Map::new();
    entry.insert("currency".into(), json!(currency));
    dates(&mut entry, "", period);
    if let Some(per) = per {
        entry.insert("average".into(), json!(per.word()));
    }
    entry
}

/// A row's name, as the book writes it and made harmless for the model
/// (`unsorted` for money with no category yet), and its category's id.
fn name(row: &CategoryRow, writer: &mut Writer<'_>) -> Map<String, Value> {
    let mut entry = Map::new();
    if row.is_unsorted() {
        entry.insert("unsorted".into(), json!(true));
    } else {
        entry.insert("name".into(), writer.text(&row.path.join(" › ")));
    }
    if let Some(category) = row.category {
        entry.insert("category_id".into(), json!(category.to_string()));
    }
    if let Some(group) = row.group {
        entry.insert("group_id".into(), json!(group.to_string()));
    }
    entry
}

/// How many rows were left out after [`MAX_TABLE_ROWS`], when some were.
fn left_out(entry: &mut Map<String, Value>, rows: usize, writer: &mut Writer<'_>) {
    if rows > MAX_TABLE_ROWS {
        entry.insert("rows_left_out".into(), writer.count(rows - MAX_TABLE_ROWS));
    }
}

/// The value, the comparison, the change and its percent, the direction and
/// the colour the table shows (D-760): green for less spending, red for
/// more, none for no change or one that reaches fewer of `CHANGE_STEPS`
/// than `neutral` asks.
fn put_change(
    row: &mut Map<String, Value>,
    change: &Change,
    neutral: usize,
    writer: &mut Writer<'_>,
) -> Result<(), ToolError> {
    row.insert("value".into(), writer.amount(change.current)?);
    row.insert("previous".into(), writer.amount(change.previous)?);
    row.insert("change".into(), writer.amount(change.amount)?);
    if let Some(percent) = change.percent {
        writer.figures.add_decimal(percent.tenths(), 1);
        row.insert(
            "change_percent".into(),
            json!(decimal_text(percent.tenths(), 1)),
        );
    }
    let (direction, colour) = match change.moved() {
        core::cmp::Ordering::Greater => ("more", "red"),
        core::cmp::Ordering::Less => ("less", "green"),
        core::cmp::Ordering::Equal => ("same", "none"),
    };
    debug_assert!(neutral <= CHANGE_STEPS.len());
    let colour = if change.reach() < neutral {
        "none"
    } else {
        colour
    };
    row.insert("direction".into(), json!(direction));
    row.insert("colour".into(), json!(colour));
    Ok(())
}
