// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cuts a formula into tokens (D-292, D-295).
//!
//! The formula is untrusted text: its length is checked before anything
//! else, the tokens are counted as they are made, and every mistake is an
//! error code with the character position, counted from 1.

use kassensturz_core::Date;

use super::exact::{Exact, MAX_SCALE};
use super::{MAX_FORMULA_CHARS, MAX_FORMULA_TOKENS, NumberForm};
use crate::{Period, ReportError};

/// How separators and decimal marks are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Form {
    /// The decimal mark.
    decimal: char,
    /// What separates the arguments of a call.
    separators: &'static [char],
}

impl Form {
    /// The form a book stores: `;` between arguments, `.` as the decimal
    /// mark, as OpenDocument files store formulas (D-295).
    pub(crate) const CANONICAL: Self = Self {
        decimal: '.',
        separators: &[';'],
    };

    /// The form the user types and reads in their number format: with a
    /// decimal comma `;` and `,`; with a decimal point `,` and `.`, and `;`
    /// is taken as a separator in every format, as LibreOffice takes it.
    pub(crate) const fn of(form: NumberForm) -> Self {
        match form {
            NumberForm::DecimalComma => Self {
                decimal: ',',
                separators: &[';'],
            },
            NumberForm::DecimalPoint => Self {
                decimal: '.',
                separators: &[',', ';'],
            },
        }
    }

    /// The decimal mark.
    pub(crate) const fn decimal(self) -> char {
        self.decimal
    }

    /// The separator this form writes.
    pub(crate) const fn separator(self) -> char {
        self.separators[0]
    }
}

/// A comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Comparison {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
}

/// What a token is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A number as written, with the byte positions of its decimal mark.
    Number(Exact, Option<usize>),
    /// A bare name such as `Pets`, which is a function's when a `(`
    /// follows.
    Name(String),
    /// A name in single quotes, with `''` read as one quote.
    Quoted(String),
    /// Text in double quotes: the words of a period.
    Text(String),
    /// The dates after `@`, already a period.
    Dates(Period),
    At,
    Plus,
    Minus,
    Times,
    Divide,
    Percent,
    Open,
    Close,
    Separator,
    Compare(Comparison),
}

/// A token and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: Kind,
    /// Its first character's position, counted from 1.
    pub(crate) at: usize,
    /// Its bytes in the text.
    pub(crate) start: usize,
    pub(crate) end: usize,
}

fn syntax(at: usize, reason: &'static str) -> ReportError {
    ReportError::FormulaSyntax {
        position: at,
        reason,
    }
}

/// The characters of a formula with their byte and character positions.
struct Reader<'a> {
    text: &'a str,
    chars: Vec<(usize, char)>,
    next: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.next).map(|&(_, c)| c)
    }

    fn peek_at(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.next + ahead).map(|&(_, c)| c)
    }

    fn byte(&self) -> usize {
        self.chars
            .get(self.next)
            .map_or(self.text.len(), |&(byte, _)| byte)
    }

    /// The position of the next character, counted from 1.
    fn at(&self) -> usize {
        self.next + 1
    }

    fn take_digits(&mut self, at_most: usize) -> &str {
        let start = self.byte();
        let mut taken = 0;
        while taken < at_most && self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.next += 1;
            taken += 1;
        }
        &self.text[start..self.byte()]
    }
}

/// Cuts `text` into tokens in `form`. Fails with `report.formula-too-big`
/// over [`MAX_FORMULA_CHARS`] characters or [`MAX_FORMULA_TOKENS`] tokens, and with a
/// syntax error for what is no token.
pub(crate) fn tokens(text: &str, form: Form) -> Result<Vec<Token>, ReportError> {
    let chars: Vec<(usize, char)> = text.char_indices().take(MAX_FORMULA_CHARS + 1).collect();
    if chars.len() > MAX_FORMULA_CHARS {
        return Err(ReportError::FormulaTooBig {
            limit: "characters",
        });
    }
    let mut reader = Reader {
        text,
        chars,
        next: 0,
    };
    let mut tokens: Vec<Token> = Vec::new();
    while let Some(c) = reader.peek() {
        if c.is_whitespace() {
            reader.next += 1;
            continue;
        }
        if tokens.len() == MAX_FORMULA_TOKENS {
            return Err(ReportError::FormulaTooBig { limit: "tokens" });
        }
        let at = reader.at();
        let start = reader.byte();
        let after_at = tokens.last().is_some_and(|token| token.kind == Kind::At);
        let kind = if c.is_ascii_digit() && after_at {
            Kind::Dates(dates(&mut reader)?)
        } else if c.is_ascii_digit() {
            number(&mut reader, form)?
        } else if c.is_alphabetic() {
            reader.next += 1;
            while reader
                .peek()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                reader.next += 1;
            }
            Kind::Name(text[start..reader.byte()].to_owned())
        } else if c == '\'' || c == '"' {
            quoted(&mut reader, c)?
        } else {
            reader.next += 1;
            let mut two = |second: char, kind: Kind| {
                (reader.peek() == Some(second)).then(|| {
                    reader.next += 1;
                    kind
                })
            };
            match c {
                '+' => Kind::Plus,
                '-' => Kind::Minus,
                '*' => Kind::Times,
                '/' => Kind::Divide,
                '%' => Kind::Percent,
                '(' => Kind::Open,
                ')' => Kind::Close,
                '@' => Kind::At,
                '=' => Kind::Compare(Comparison::Equal),
                '<' => two('>', Kind::Compare(Comparison::NotEqual))
                    .or_else(|| two('=', Kind::Compare(Comparison::LessOrEqual)))
                    .unwrap_or(Kind::Compare(Comparison::Less)),
                '>' => two('=', Kind::Compare(Comparison::GreaterOrEqual))
                    .unwrap_or(Kind::Compare(Comparison::Greater)),
                c if form.separators.contains(&c) => Kind::Separator,
                ',' | ';' => return Err(syntax(at, "wrong-separator")),
                c if c == form.decimal => return Err(syntax(at, "bad-number")),
                _ => return Err(syntax(at, "bad-character")),
            }
        };
        tokens.push(Token {
            kind,
            at,
            start,
            end: reader.byte(),
        });
    }
    Ok(tokens)
}

/// `digits [decimal digits]`, with no grouping and no exponent.
fn number(reader: &mut Reader<'_>, form: Form) -> Result<Kind, ReportError> {
    let at = reader.at();
    let whole = reader.take_digits(usize::MAX).to_owned();
    let mut fraction = String::new();
    let mut mark = None;
    // A mark with no digit after it ends the number, and is read on its
    // own: `12.` is a bad number, `SUM(1, 2)` with a decimal comma a wrong
    // separator.
    if reader.peek() == Some(form.decimal) && reader.peek_at(1).is_some_and(|c| c.is_ascii_digit())
    {
        mark = Some(reader.byte());
        reader.next += 1;
        fraction = reader.take_digits(usize::MAX).to_owned();
    }
    let scale = u32::try_from(fraction.len()).unwrap_or(u32::MAX);
    let units = format!("{whole}{fraction}")
        .parse::<i128>()
        .map_err(|_| syntax(at, "bad-number"))?;
    if scale > MAX_SCALE {
        return Err(syntax(at, "bad-number"));
    }
    let value = Exact::new(units, scale).map_err(|_| syntax(at, "bad-number"))?;
    Ok(Kind::Number(value, mark))
}

/// A name in single quotes or the words of a period in double quotes, the
/// quote written twice inside.
fn quoted(reader: &mut Reader<'_>, quote: char) -> Result<Kind, ReportError> {
    let at = reader.at();
    reader.next += 1;
    let mut content = String::new();
    loop {
        match reader.peek() {
            None => return Err(syntax(at, "missing-quote")),
            Some(c) if c == quote => {
                reader.next += 1;
                if reader.peek() == Some(quote) {
                    reader.next += 1;
                    content.push(quote);
                } else {
                    break;
                }
            }
            Some(c) => {
                reader.next += 1;
                content.push(c);
            }
        }
    }
    Ok(if quote == '\'' {
        Kind::Quoted(content)
    } else {
        Kind::Text(content)
    })
}

/// One end of a range of dates: `2026`, `2026-03` or `2026-03-15`, as its
/// first and last day.
fn date_end(reader: &mut Reader<'_>) -> Result<(Date, Date), ReportError> {
    let at = reader.at();
    let bad = || syntax(at, "bad-period");
    let number = |digits: &str| digits.parse::<i16>().map_err(|_| bad());
    let year = reader.take_digits(4).to_owned();
    if year.len() != 4 {
        return Err(bad());
    }
    let year = number(&year)?;
    let part = |reader: &mut Reader<'_>| -> Result<Option<i8>, ReportError> {
        if reader.peek() != Some('-') || !reader.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            return Ok(None);
        }
        reader.next += 1;
        let digits = reader.take_digits(2).to_owned();
        if digits.len() != 2 || reader.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(bad());
        }
        let value = number(&digits)?;
        i8::try_from(value).map(Some).map_err(|_| bad())
    };
    let first_year = Date::new(year, 1, 1).map_err(|_| bad())?;
    let Some(month) = part(reader)? else {
        return Ok((first_year, first_year.last_of_year()));
    };
    let first_month = Date::new(year, month, 1).map_err(|_| bad())?;
    let Some(day) = part(reader)? else {
        return Ok((first_month, first_month.last_of_month()));
    };
    let day = Date::new(year, month, day).map_err(|_| bad())?;
    Ok((day, day))
}

/// The dates after `@`: one end, or two joined by `..`, both included.
fn dates(reader: &mut Reader<'_>) -> Result<Period, ReportError> {
    let (first, mut last) = date_end(reader)?;
    if reader.peek() == Some('.') && reader.peek_at(1) == Some('.') {
        reader.next += 2;
        (_, last) = date_end(reader)?;
    }
    if reader
        .peek()
        .is_some_and(|c| c.is_alphanumeric() || c == '.')
    {
        return Err(syntax(reader.at(), "bad-period"));
    }
    Period::new(first, last)
}
