// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads the tokens into a tree and works out the kind of every part
//! before any figure is read (D-292).
//!
//! The grammar of `docs/REPORT_DESIGNER_M6B.md` §5, by recursive descent.
//! Only brackets and calls nest, and each counts against [`MAX_FORMULA_DEPTH`], so
//! a hostile formula can't run the parser deep; chains of `+` or `*` and
//! runs of signs are read in loops.

use std::collections::HashSet;

use super::exact::Exact;
use super::lex::{Comparison, Kind as Token, Token as Lexed};
use super::{MAX_FORMULA_ARGUMENTS, MAX_FORMULA_DEPTH, MAX_FORMULA_NAMES, MAX_FORMULA_PERIODS};
use crate::source::Kind as Side;
use crate::{Period, ReportError};

/// The words of a period, counted from the figure's period (D-292).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Words {
    ThisMonth,
    LastMonth,
    ThisQuarter,
    LastQuarter,
    ThisYear,
    LastYear,
    /// `"this week"`: the week the figure's last day is in, starting on the
    /// locale's first day of the week ([`crate::first_weekday`], D-740).
    ThisWeek,
    /// `"last week"`: the whole week before it.
    LastWeek,
    /// `"last N months"` (the figure's month and the N-1 before it, so
    /// `"last 12 months"` as before) or `"last N complete months"` (the N
    /// whole months before the figure's month), N from 1 to
    /// [`crate::MAX_ROLLING_MONTHS`] (US1.13, D-649; the words of D-644).
    Months {
        count: u16,
        complete: bool,
    },
    /// `"N months ago"`: the one calendar month N months before the
    /// figure's month, N from 1 to [`crate::MAX_ROLLING_MONTHS`] (US5.6,
    /// D-740): `"2 months ago"` is the month before last.
    MonthsAgo {
        count: u16,
    },
    /// `"last N weeks"` (the figure's week and the N-1 before it) or
    /// `"last N complete weeks"` (the N whole weeks before it), N from 1 to
    /// [`crate::MAX_FORMULA_WEEKS`] (US5.6, D-740).
    Weeks {
        count: u16,
        complete: bool,
    },
    /// `"last N years"` (the figure's calendar year and the N-1 before it)
    /// or `"last N complete years"` (the N whole years before it), N from 1
    /// to [`crate::MAX_FORMULA_YEARS`] (US5.6, D-740).
    Years {
        count: u16,
        complete: bool,
    },
    /// `"last N days"`: the figure's last day and the N-1 days before it,
    /// N from 1 to [`crate::MAX_ROLLING_DAYS`], as the period box counts
    /// them (US4.7, D-717; added to formulas by Q-499).
    Days {
        count: u16,
    },
}

/// Why the words after `@` are no period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotWords {
    /// Not words formulas know.
    Unknown,
    /// A form formulas know, with a number outside 1 to `most` or written
    /// with a leading zero; `unit` is `days`, `weeks`, `months` or `years`
    /// (#759 point 3).
    Count { unit: &'static str, most: u16 },
}

impl Words {
    fn read(text: &str) -> Result<Self, NotWords> {
        let words = text
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" ");
        Ok(match words.as_str() {
            "this month" => Self::ThisMonth,
            "last month" => Self::LastMonth,
            "this quarter" => Self::ThisQuarter,
            "last quarter" => Self::LastQuarter,
            "this year" => Self::ThisYear,
            "last year" => Self::LastYear,
            "this week" => Self::ThisWeek,
            "last week" => Self::LastWeek,
            _ => return Self::counted(&words),
        })
    }

    /// The forms with a number N: `last N days`, `last N [complete] weeks`,
    /// `last N [complete] months`, `last N [complete] years` and
    /// `N months ago`. N is written in digits without a leading zero, so
    /// each period is written one way; for N = 1 the unit may be singular
    /// (`last 1 month`, `1 month ago`).
    fn counted(words: &str) -> Result<Self, NotWords> {
        let (number, unit, ago) = if let Some(rest) = words.strip_prefix("last ") {
            let Some((number, unit)) = rest.split_once(' ') else {
                return Err(NotWords::Unknown);
            };
            (number, unit, false)
        } else if let Some(rest) = words.strip_suffix(" ago") {
            let Some((number, unit)) = rest.split_once(' ') else {
                return Err(NotWords::Unknown);
            };
            (number, unit, true)
        } else {
            return Err(NotWords::Unknown);
        };
        let (complete, unit) = match unit.strip_prefix("complete ") {
            Some(unit) if !ago => (true, unit),
            _ => (false, unit),
        };
        let (unit, singular, most) = match unit {
            "days" | "day" if !complete && !ago => ("days", unit == "day", crate::MAX_ROLLING_DAYS),
            "weeks" | "week" if !ago => ("weeks", unit == "week", crate::MAX_FORMULA_WEEKS),
            "months" | "month" => ("months", unit == "month", crate::MAX_ROLLING_MONTHS),
            "years" | "year" if !ago => ("years", unit == "year", crate::MAX_FORMULA_YEARS),
            _ => return Err(NotWords::Unknown),
        };
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(NotWords::Unknown);
        }
        let refused = NotWords::Count { unit, most };
        if number.starts_with('0') {
            return Err(refused);
        }
        let count = number
            .parse::<u16>()
            .ok()
            .filter(|count| (1..=most).contains(count))
            .ok_or(refused)?;
        if singular && count != 1 {
            return Err(NotWords::Unknown);
        }
        Ok(match (unit, ago) {
            ("months", true) => Self::MonthsAgo { count },
            ("days", _) => Self::Days { count },
            ("weeks", _) => Self::Weeks { count, complete },
            ("years", _) => Self::Years { count, complete },
            _ => Self::Months { count, complete },
        })
    }
}

/// The range a name gives itself with `@`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Range {
    Words(Words),
    Dates(Period),
}

/// The wave 1 functions (D-292).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Function {
    Sum,
    Average,
    Min,
    Max,
    Count,
    Abs,
    Sign,
    Round,
    RoundUp,
    RoundDown,
    Int,
    If,
    And,
    Or,
    Not,
    IfError,
    PercentOf,
    Change,
}

impl Function {
    pub(super) const ALL: [Self; 18] = [
        Self::Sum,
        Self::Average,
        Self::Min,
        Self::Max,
        Self::Count,
        Self::Abs,
        Self::Sign,
        Self::Round,
        Self::RoundUp,
        Self::RoundDown,
        Self::Int,
        Self::If,
        Self::And,
        Self::Or,
        Self::Not,
        Self::IfError,
        Self::PercentOf,
        Self::Change,
    ];

    /// The English name a formula writes (Q-226 = A).
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Sum => "SUM",
            Self::Average => "AVERAGE",
            Self::Min => "MIN",
            Self::Max => "MAX",
            Self::Count => "COUNT",
            Self::Abs => "ABS",
            Self::Sign => "SIGN",
            Self::Round => "ROUND",
            Self::RoundUp => "ROUNDUP",
            Self::RoundDown => "ROUNDDOWN",
            Self::Int => "INT",
            Self::If => "IF",
            Self::And => "AND",
            Self::Or => "OR",
            Self::Not => "NOT",
            Self::IfError => "IFERROR",
            Self::PercentOf => "PERCENT_OF",
            Self::Change => "CHANGE",
        }
    }

    /// The fewest and the most arguments.
    const fn arguments(self) -> (usize, usize) {
        match self {
            Self::Sum | Self::Average | Self::Min | Self::Max | Self::Count => {
                (1, MAX_FORMULA_ARGUMENTS)
            }
            Self::And | Self::Or => (1, MAX_FORMULA_ARGUMENTS),
            Self::Abs | Self::Sign | Self::Int | Self::Not => (1, 1),
            // The places are optional, 0 when left out, as in spreadsheets.
            Self::Round | Self::RoundUp | Self::RoundDown => (1, 2),
            Self::IfError | Self::PercentOf | Self::Change => (2, 2),
            Self::If => (3, 3),
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|function| function.name().eq_ignore_ascii_case(name))
    }
}

/// `+ - * /`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// A part of a formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Node {
    Number(Exact),
    /// The name with this index in [`Tree::names`].
    Name(usize),
    Negate(Box<Expr>),
    Percent(Box<Expr>),
    Binary(Operator, Box<Expr>, Box<Expr>),
    Compare(Comparison, Box<Expr>, Box<Expr>),
    Call(Function, Vec<Expr>),
}

/// A part and where it starts, counted in characters from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Expr {
    pub(crate) node: Node,
    pub(crate) at: usize,
}

/// A name as the formula writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Name {
    pub(crate) text: String,
    pub(crate) range: Option<Range>,
    pub(crate) at: usize,
    /// Only an argument of `COUNT`, so its money doesn't decide the
    /// formula's currency.
    pub(crate) only_counted: bool,
    /// `Income.` or `Spending.` in front: only an income or only a
    /// spending category or group fits (D-653).
    pub(crate) side: Option<Side>,
    /// `Income.ALL` or `Spending.ALL`: everything of that side, also what
    /// is not sorted into a category yet, and not a name to look up.
    pub(crate) all: bool,
}

impl Name {
    /// The name as an error message shows it: with its kind in front when
    /// the formula gave one.
    pub(crate) fn shown(&self) -> String {
        match self.side {
            Some(side) => format!("{}.{}", side_word(side), self.text),
            None => self.text.clone(),
        }
    }
}

/// The word a formula writes for a kind (D-653).
pub(crate) const fn side_word(side: Side) -> &'static str {
    match side {
        Side::Income => "Income",
        Side::Spending => "Spending",
    }
}

fn read_side(word: &str) -> Option<Side> {
    [Side::Income, Side::Spending]
        .into_iter()
        .find(|&side| side_word(side).eq_ignore_ascii_case(word))
}

/// The kind of a value, known before any figure is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Amount,
    /// A number; `percent` when it is shown as a percent.
    Number {
        percent: bool,
    },
    YesNo,
}

/// A formula read into a tree, with its names and its result's kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tree {
    pub(crate) root: Expr,
    pub(crate) names: Vec<Name>,
    pub(crate) kind: Kind,
}

fn syntax(at: usize, reason: &'static str) -> ReportError {
    ReportError::FormulaSyntax {
        position: at,
        reason,
    }
}

fn kinds(at: usize, reason: &'static str) -> ReportError {
    ReportError::FormulaKinds {
        position: at,
        reason,
    }
}

struct Parser<'a> {
    tokens: &'a [Lexed],
    next: usize,
    /// The position after the last character, where a missing part is.
    end: usize,
    depth: usize,
    names: Vec<Name>,
    ranges: HashSet<Range>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.next).map(|token| &token.kind)
    }

    fn at(&self) -> usize {
        self.tokens
            .get(self.next)
            .map_or(self.end, |token| token.at)
    }

    fn take(&mut self) -> Option<&Lexed> {
        let token = self.tokens.get(self.next)?;
        self.next += 1;
        Some(token)
    }

    fn deeper(&mut self) -> Result<(), ReportError> {
        self.depth += 1;
        if self.depth > MAX_FORMULA_DEPTH {
            return Err(ReportError::FormulaTooBig { limit: "depth" });
        }
        Ok(())
    }

    /// `sum [comparison sum]`: one comparison at most.
    fn test(&mut self) -> Result<Expr, ReportError> {
        let left = self.sum()?;
        let Some(&Token::Compare(comparison)) = self.peek() else {
            return Ok(left);
        };
        let at = self.at();
        self.next += 1;
        let right = self.sum()?;
        if let Some(Token::Compare(_)) = self.peek() {
            return Err(syntax(self.at(), "two-comparisons"));
        }
        Ok(Expr {
            node: Node::Compare(comparison, Box::new(left), Box::new(right)),
            at,
        })
    }

    fn sum(&mut self) -> Result<Expr, ReportError> {
        let mut left = self.product()?;
        loop {
            let operator = match self.peek() {
                Some(Token::Plus) => Operator::Add,
                Some(Token::Minus) => Operator::Subtract,
                _ => return Ok(left),
            };
            let at = self.at();
            self.next += 1;
            let right = self.product()?;
            left = Expr {
                node: Node::Binary(operator, Box::new(left), Box::new(right)),
                at,
            };
        }
    }

    fn product(&mut self) -> Result<Expr, ReportError> {
        let mut left = self.unary()?;
        loop {
            let operator = match self.peek() {
                Some(Token::Times) => Operator::Multiply,
                Some(Token::Divide) => Operator::Divide,
                _ => return Ok(left),
            };
            let at = self.at();
            self.next += 1;
            let right = self.unary()?;
            left = Expr {
                node: Node::Binary(operator, Box::new(left), Box::new(right)),
                at,
            };
        }
    }

    /// Any number of signs, read in a loop, then `primary [%]`.
    fn unary(&mut self) -> Result<Expr, ReportError> {
        let mut negate = None;
        loop {
            match self.peek() {
                Some(Token::Minus) => {
                    negate = match negate {
                        None => Some(self.at()),
                        Some(_) => None,
                    };
                }
                Some(Token::Plus) => {}
                _ => break,
            }
            self.next += 1;
        }
        let mut value = self.primary()?;
        if let Some(Token::Percent) = self.peek() {
            let at = self.at();
            self.next += 1;
            value = Expr {
                node: Node::Percent(Box::new(value)),
                at,
            };
        }
        Ok(match negate {
            Some(at) => Expr {
                node: Node::Negate(Box::new(value)),
                at,
            },
            None => value,
        })
    }

    fn primary(&mut self) -> Result<Expr, ReportError> {
        let at = self.at();
        let Some(token) = self.take() else {
            return Err(syntax(at, "expected-value"));
        };
        match token.kind.clone() {
            Token::Number(value, _) => Ok(Expr {
                node: Node::Number(value),
                at,
            }),
            Token::Name(name) if self.peek() == Some(&Token::Open) => self.call(&name, at),
            Token::Name(word) if self.peek() == Some(&Token::Dot) => self.kinded(&word, at),
            Token::Name(text) | Token::Quoted(text) => self.name(text, at, None, false),
            Token::Open => {
                self.deeper()?;
                let inside = self.test()?;
                if self.peek() != Some(&Token::Close) {
                    return Err(syntax(self.at(), "missing-close"));
                }
                self.next += 1;
                self.depth -= 1;
                Ok(inside)
            }
            Token::Separator => Err(syntax(at, "wrong-separator")),
            _ => Err(syntax(at, "expected-value")),
        }
    }

    /// `Income . name` or `Spending . name`, also `Spending . ALL` (D-653).
    /// `ALL` bare is the term; `'All'` in quotes is a name like any other.
    fn kinded(&mut self, word: &str, at: usize) -> Result<Expr, ReportError> {
        let side = read_side(word).ok_or_else(|| syntax(at, "unknown-kind"))?;
        // The `.`.
        self.next += 1;
        let after = self.at();
        match self.take().map(|token| token.kind.clone()) {
            Some(Token::Name(text)) if text.eq_ignore_ascii_case("all") => {
                self.name(text, at, Some(side), true)
            }
            Some(Token::Name(text) | Token::Quoted(text)) => self.name(text, at, Some(side), false),
            _ => Err(syntax(after, "expected-value")),
        }
    }

    /// `name [@ period]`.
    fn name(
        &mut self,
        text: String,
        at: usize,
        side: Option<Side>,
        all: bool,
    ) -> Result<Expr, ReportError> {
        let mut range = None;
        if self.peek() == Some(&Token::At) {
            self.next += 1;
            let period_at = self.at();
            range = Some(match self.take().map(|token| token.kind.clone()) {
                Some(Token::Dates(period)) => Range::Dates(period),
                Some(Token::Text(words)) => {
                    Range::Words(Words::read(&words).map_err(|not| match not {
                        NotWords::Unknown => syntax(period_at, "bad-period"),
                        NotWords::Count { unit, most } => ReportError::FormulaPeriodCount {
                            position: period_at,
                            unit,
                            most,
                        },
                    })?)
                }
                _ => return Err(syntax(period_at, "bad-period")),
            });
        }
        if self.names.len() == MAX_FORMULA_NAMES {
            return Err(ReportError::FormulaTooBig { limit: "names" });
        }
        if let Some(range) = range {
            self.ranges.insert(range);
            if self.ranges.len() > MAX_FORMULA_PERIODS {
                return Err(ReportError::FormulaTooBig { limit: "periods" });
            }
        }
        self.names.push(Name {
            text,
            range,
            at,
            only_counted: false,
            side,
            all,
        });
        Ok(Expr {
            node: Node::Name(self.names.len() - 1),
            at,
        })
    }

    /// `FUNCTION ( [test {; test}] )`.
    fn call(&mut self, name: &str, at: usize) -> Result<Expr, ReportError> {
        let function =
            Function::named(name).ok_or_else(|| ReportError::FormulaUnknownFunction {
                position: at,
                name: name.to_owned(),
            })?;
        self.deeper()?;
        // The `(`.
        self.next += 1;
        let mut arguments = Vec::new();
        if self.peek() == Some(&Token::Close) {
            self.next += 1;
        } else {
            loop {
                if arguments.len() == MAX_FORMULA_ARGUMENTS {
                    return Err(ReportError::FormulaTooBig { limit: "arguments" });
                }
                arguments.push(self.test()?);
                match self.peek() {
                    Some(Token::Separator) => self.next += 1,
                    Some(Token::Close) => {
                        self.next += 1;
                        break;
                    }
                    _ => return Err(syntax(self.at(), "missing-close")),
                }
            }
        }
        self.depth -= 1;
        let (fewest, most) = function.arguments();
        let wrong = |reason| ReportError::FormulaArguments {
            position: at,
            function: function.name(),
            reason,
        };
        if arguments.len() < fewest {
            return Err(wrong("too-few"));
        }
        if arguments.len() > most {
            return Err(wrong("too-many"));
        }
        if function == Function::Count {
            for argument in &arguments {
                let Node::Name(index) = argument.node else {
                    return Err(ReportError::FormulaArguments {
                        position: argument.at,
                        function: function.name(),
                        reason: "names-only",
                    });
                };
                self.names[index].only_counted = true;
            }
        }
        Ok(Expr {
            node: Node::Call(function, arguments),
            at,
        })
    }
}

/// Reads `tokens` into a tree and works out its kinds. `end` is the
/// position after the formula's last character.
pub(crate) fn tree(tokens: &[Lexed], end: usize) -> Result<Tree, ReportError> {
    let mut parser = Parser {
        tokens,
        next: 0,
        end,
        depth: 0,
        names: Vec::new(),
        ranges: HashSet::new(),
    };
    // The `=` in front is optional.
    if let Some(Token::Compare(Comparison::Equal)) = parser.peek() {
        parser.next += 1;
    }
    if parser.peek().is_none() {
        return Err(syntax(parser.at(), "empty"));
    }
    let root = parser.test()?;
    if let Some(token) = parser.peek() {
        let reason = match token {
            Token::Separator => "wrong-separator",
            Token::Close => "missing-open",
            _ => "unexpected",
        };
        return Err(syntax(parser.at(), reason));
    }
    let names = parser.names;
    let kind = kind_of(&root)?;
    if kind == Kind::YesNo {
        return Err(kinds(root.at, "yes-no-result"));
    }
    Ok(Tree { root, names, kind })
}

/// A value's kind, or `yes-no-in-arithmetic` where a yes/no can't be.
fn value(expr: &Expr) -> Result<Kind, ReportError> {
    match kind_of(expr)? {
        Kind::YesNo => Err(kinds(expr.at, "yes-no-in-arithmetic")),
        kind => Ok(kind),
    }
}

/// The kind two values give together where a number joins an amount as an
/// amount in its currency (`Pets - 50`, `IF(test; Pets; 0)`); a number is
/// shown as a percent when both are.
fn joined(left: Kind, right: Kind) -> Kind {
    match (left, right) {
        (Kind::Number { percent: left }, Kind::Number { percent: right }) => Kind::Number {
            percent: left && right,
        },
        _ => Kind::Amount,
    }
}

/// The kind of `expr`, or why its parts don't go together.
fn kind_of(expr: &Expr) -> Result<Kind, ReportError> {
    let plain = Kind::Number { percent: false };
    Ok(match &expr.node {
        Node::Number(_) => plain,
        Node::Name(_) => Kind::Amount,
        Node::Negate(inside) => value(inside)?,
        Node::Percent(inside) => match value(inside)? {
            Kind::Amount => Kind::Amount,
            _ => Kind::Number { percent: true },
        },
        Node::Binary(operator, left, right) => {
            let (left, right) = (value(left)?, value(right)?);
            match (operator, left, right) {
                (Operator::Add | Operator::Subtract, Kind::Number { .. }, Kind::Number { .. }) => {
                    plain
                }
                (Operator::Add | Operator::Subtract, _, _) => Kind::Amount,
                (Operator::Multiply, Kind::Amount, Kind::Amount) => {
                    return Err(kinds(expr.at, "amount-times-amount"));
                }
                (Operator::Multiply, Kind::Number { .. }, Kind::Number { .. }) => plain,
                (Operator::Multiply, _, _) => Kind::Amount,
                (Operator::Divide, Kind::Amount, Kind::Amount) => plain,
                (Operator::Divide, Kind::Amount, _) => Kind::Amount,
                (Operator::Divide, _, Kind::Amount) => {
                    return Err(kinds(expr.at, "number-over-amount"));
                }
                (Operator::Divide, _, _) => plain,
            }
        }
        Node::Compare(_, left, right) => {
            value(left)?;
            value(right)?;
            Kind::YesNo
        }
        Node::Call(function, arguments) => call_kind(*function, arguments)?,
    })
}

fn call_kind(function: Function, arguments: &[Expr]) -> Result<Kind, ReportError> {
    let plain = Kind::Number { percent: false };
    let test = |expr: &Expr| match kind_of(expr)? {
        Kind::YesNo => Ok(()),
        _ => Err(kinds(expr.at, "needs-yes-no")),
    };
    // Two branches: both yes/no, or both values.
    let branches = |yes: &Expr, no: &Expr| match (kind_of(yes)?, kind_of(no)?) {
        (Kind::YesNo, Kind::YesNo) => Ok(Kind::YesNo),
        (Kind::YesNo, _) => Err(kinds(no.at, "if-kinds-differ")),
        (_, Kind::YesNo) => Err(kinds(no.at, "if-kinds-differ")),
        (yes, no) => Ok(joined(yes, no)),
    };
    Ok(match function {
        Function::Sum | Function::Average | Function::Min | Function::Max => {
            let mut kind = None;
            for argument in arguments {
                let next = value(argument)?;
                kind = Some(kind.map_or(next, |kind| joined(kind, next)));
            }
            match kind {
                // A sum of percents is no percent any more.
                Some(Kind::Number { percent: true }) if function == Function::Sum => plain,
                Some(kind) => kind,
                None => plain,
            }
        }
        Function::Count => plain,
        Function::Abs | Function::Int => value(&arguments[0])?,
        Function::Sign => {
            value(&arguments[0])?;
            plain
        }
        Function::Round | Function::RoundUp | Function::RoundDown => {
            let kind = value(&arguments[0])?;
            if let Some(places) = arguments.get(1)
                && value(places)? == Kind::Amount
            {
                return Err(ReportError::FormulaArguments {
                    position: places.at,
                    function: function.name(),
                    reason: "places",
                });
            }
            kind
        }
        Function::If => {
            test(&arguments[0])?;
            branches(&arguments[1], &arguments[2])?
        }
        Function::IfError => branches(&arguments[0], &arguments[1])?,
        Function::And | Function::Or | Function::Not => {
            for argument in arguments {
                test(argument)?;
            }
            Kind::YesNo
        }
        Function::PercentOf | Function::Change => {
            match (value(&arguments[0])?, value(&arguments[1])?) {
                (Kind::Amount, Kind::Amount) | (Kind::Number { .. }, Kind::Number { .. }) => {}
                _ => return Err(kinds(arguments[1].at, "ratio-kinds")),
            }
            Kind::Number { percent: true }
        }
    })
}
