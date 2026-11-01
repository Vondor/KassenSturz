// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Formulas: a figure the user writes, spreadsheet style (M6b.6, D-292).
//!
//! `=SUM(Pets; Travel)`, `=Pets - Travel`, `=PERCENT_OF(Groceries; Food)`.
//! The syntax follows the public descriptions of spreadsheet formulas
//! (OpenDocument, LibreOffice's help), names and argument order only; no
//! code of any spreadsheet is used (AGENTS.md §3.1 rule 5). The language is
//! closed and small: a name is a category or one of My groups and stands
//! for its figure in a period; a formula reads nothing else, writes nothing
//! and can't refer to another formula, so it can't loop.
//!
//! Every value is exact ([`exact`], D-317); division is the only rounding
//! inside, and the result is rounded once at the end. A formula is
//! untrusted input: its size is checked against fixed limits before
//! anything is read, and every mistake is an error code with the position
//! of the character it is at (D-034).
//!
//! The book keeps a formula in one canonical form (`;` between arguments,
//! `.` as the decimal mark); the formula field shows and takes the form of
//! the user's number format, and [`Formula::to_canonical`] and
//! [`Formula::to_users_form`] convert token by token (D-295).

mod eval;
mod exact;
mod lex;
mod names;
mod parse;

pub use names::{FUNCTIONS, FunctionInfo, KnownName};

use kassensturz_core::{CurrencyCode, GroupMember, Money, RoundingMode};

use crate::{Filter, Percent, Period, ReportError, Source};
use lex::{Form, Kind as Token};

/// The most characters a formula may have.
pub const MAX_FORMULA_CHARS: usize = 1000;
/// The most tokens (names, numbers, signs, brackets) a formula may have.
pub const MAX_FORMULA_TOKENS: usize = 500;
/// How deep brackets and calls may nest.
pub const MAX_FORMULA_DEPTH: usize = 16;
/// The most arguments a call may have.
pub const MAX_FORMULA_ARGUMENTS: usize = 30;
/// The most names a formula may hold.
pub const MAX_FORMULA_NAMES: usize = 64;
/// The most different periods a formula's names may give themselves.
pub const MAX_FORMULA_PERIODS: usize = 8;

/// How the user's number format writes numbers, and so formulas (D-295).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberForm {
    /// A decimal comma, as in German: `=ROUND(Pets * 1,5; 2)`.
    DecimalComma,
    /// A decimal point, as in English: `=ROUND(Pets * 1.5, 2)`; `;` is a
    /// separator too.
    DecimalPoint,
}

/// A formula, read and checked: its syntax, its limits and the kinds of its
/// parts. The names are looked up only when it is worked out, against the
/// book of the moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formula {
    tree: parse::Tree,
}

/// What a formula gives, rounded once (D-292).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormulaValue {
    /// An amount, rounded to its currency's minor unit by the book's rule.
    Amount(Money),
    /// A number shown as a percent, to one decimal place, half away from
    /// zero.
    Percent(Percent),
    /// Any other number, to two decimal places, half away from zero: 12.5
    /// is 1250 hundredths.
    Number {
        /// The number times 100.
        hundredths: i64,
    },
}

impl FormulaValue {
    /// How full a progress bar is when `self` is the figure and `target`
    /// what a full bar stands for: `self` as a percent of `target`, to one
    /// decimal place, half away from zero, as the cards' ratios are (D-285).
    /// Both are amounts in one currency, both percents or both numbers;
    /// anything else, a target of zero or a ratio too large to keep is
    /// `report.formula-target`.
    pub fn fill(self, target: Self) -> Result<Percent, ReportError> {
        let (figure, whole) = match (self, target) {
            (Self::Amount(figure), Self::Amount(whole))
                if figure.currency() == whole.currency() =>
            {
                (
                    i128::from(figure.minor_units()),
                    i128::from(whole.minor_units()),
                )
            }
            (Self::Percent(figure), Self::Percent(whole)) => {
                (i128::from(figure.tenths()), i128::from(whole.tenths()))
            }
            (Self::Number { hundredths: figure }, Self::Number { hundredths: whole }) => {
                (i128::from(figure), i128::from(whole))
            }
            _ => return Err(ReportError::FormulaTarget { reason: "kinds" }),
        };
        if whole == 0 {
            return Err(ReportError::FormulaTarget { reason: "zero" });
        }
        // Tenths of a percent: figure ÷ whole × 1000, rounded once.
        RoundingMode::HalfAwayFromZero
            .divide(figure * 1_000, whole)
            .and_then(|tenths| i64::try_from(tenths).ok())
            .map(Percent::from_tenths)
            .ok_or(ReportError::FormulaTarget {
                reason: "too-large",
            })
    }
}

/// A category or group a formula counted, for the line "counts Pets (with
/// Vet), Travel" next to its result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counted {
    /// The category or group.
    pub of: GroupMember,
    /// The category's names from the top-level category down, or the
    /// group's name alone.
    pub path: Vec<String>,
    /// The names of the other categories counted with it, in the order of
    /// the category tree: what is inside a category, or the categories of a
    /// group.
    pub inside: Vec<String>,
    /// The days it was counted for.
    pub period: Period,
}

/// A formula's result and what it counted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaResult {
    /// The figure.
    pub value: FormulaValue,
    /// Each category or group with each period it was counted for, once,
    /// in the order the formula names them.
    pub counted: Vec<Counted>,
}

impl Formula {
    /// Reads a formula in the canonical form the book stores.
    pub fn parse(text: &str) -> Result<Self, ReportError> {
        Self::parse_in(text, Form::CANONICAL)
    }

    /// Reads a formula as the user typed it in their number format.
    pub fn parse_users(text: &str, form: NumberForm) -> Result<Self, ReportError> {
        Self::parse_in(text, Form::of(form))
    }

    fn parse_in(text: &str, form: Form) -> Result<Self, ReportError> {
        let tokens = lex::tokens(text, form)?;
        let end = text.chars().count() + 1;
        Ok(Self {
            tree: parse::tree(&tokens, end)?,
        })
    }

    /// The text the user typed in `form`, in the canonical form the book
    /// stores. Only separators and decimal marks change; names, quoted text
    /// and spacing stay as typed (D-295).
    pub fn to_canonical(text: &str, form: NumberForm) -> Result<String, ReportError> {
        convert(text, Form::of(form), Form::CANONICAL)
    }

    /// A stored formula in the form of the user's number format.
    pub fn to_users_form(text: &str, form: NumberForm) -> Result<String, ReportError> {
        convert(text, Form::CANONICAL, Form::of(form))
    }

    /// The names the formula writes, as they are written, without quotes
    /// (M6d.4: the summary of a dashboard file lists those its book lacks).
    pub(crate) fn name_texts(&self) -> impl Iterator<Item = &str> {
        self.tree.names.iter().map(|name| name.text.as_str())
    }

    /// The categories and groups of `source`, each written the way a
    /// formula names it, for completing a name while it is typed.
    pub fn known_names(source: &Source) -> Vec<KnownName> {
        names::known(source)
    }

    /// The days a [`Source`] must be read for to work this formula out for
    /// `period`: from the first to the last day of `period` and of every
    /// period a name gives itself, so one read serves them all.
    pub fn span(&self, period: Period) -> Result<Period, ReportError> {
        let mut first = period.first();
        let mut last = period.last();
        for name in &self.tree.names {
            let range = eval::period_of(name.range, period)?;
            first = first.min(range.first());
            last = last.max(range.last());
        }
        Period::new(first, last)
    }

    /// Works the formula out for `period`, the figure's period, from
    /// `source`, which must have been read for [`Formula::span`]. Every name
    /// counts the money that passes `filter`. `currency` is the result's
    /// currency when no name has money in the period.
    pub fn evaluate(
        &self,
        source: &Source,
        period: Period,
        filter: &Filter,
        currency: CurrencyCode,
    ) -> Result<FormulaResult, ReportError> {
        eval::evaluate(&self.tree, source, period, filter, currency)
    }
}

/// `text` written in `from`, rewritten in `to`, token by token.
fn convert(text: &str, from: Form, to: Form) -> Result<String, ReportError> {
    let tokens = lex::tokens(text, from)?;
    let mut converted = String::with_capacity(text.len());
    let mut copied = 0;
    for token in &tokens {
        let replace = match token.kind {
            Token::Separator => Some((token.start, token.end, to.separator())),
            Token::Number(_, Some(mark)) => {
                Some((mark, mark + from.decimal().len_utf8(), to.decimal()))
            }
            _ => None,
        };
        if let Some((start, end, with)) = replace {
            converted.push_str(&text[copied..start]);
            converted.push(with);
            copied = end;
        }
    }
    converted.push_str(&text[copied..]);
    Ok(converted)
}
