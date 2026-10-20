// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! OFX, Open Financial Exchange: the statement file many banks and credit
//! card companies offer next to CAMT.053 and MT940, also known as `.qfx`
//! (AGENTS.md §5). What the importer takes from the file, and how it
//! decides, is D-098 in `docs/DECISIONS.md`.
//!
//! Two syntaxes carry the same elements. OFX 1.x is SGML: a text header of
//! `KEY:VALUE` lines, then tags in which an element that holds a value is
//! usually *not* closed (`<TRNAMT>-12.50`). OFX 2.x is XML, with every
//! element closed. One small reader handles both, since a real XML parser
//! would refuse the first and the second needs nothing more: an element
//! followed by text holds that value, any other element holds elements.
//!
//! The file is untrusted (AGENTS.md §8): it is read as a stream of tags,
//! never as a tree, a document type definition or any other `<!` markup is
//! refused so no entity is ever defined or fetched, and the limits on
//! nesting, entries and text are kept while reading.

use kassensturz_core::{Currency, CurrencyCode, Date, Money};

use crate::amount::{DecimalMark, currency, parse_amount};
use crate::csv::{TextEncoding, decode};
use crate::error::ImportError;
use crate::importer::{ImportKind, Imported, Importer, ImporterInfo, Recognition};
use crate::limits::Limits;
use crate::mt940::{looks_like_iban, some_text};
use crate::statement::{Balance, Entry, Statement, StatementAccount};

/// What banks write where the format wants a transaction ID and they have
/// none, as in the other formats.
const NO_REFERENCE: [&str; 2] = ["NONREF", "NOTPROVIDED"];

/// The start of the root element, which every OFX file has.
const ROOT: &[u8] = b"<OFX>";

/// The most bytes before the root element: a header of a dozen short lines,
/// or an XML declaration and one processing instruction.
const MAX_PREAMBLE: usize = 4096;

/// The longest tag name: the longest in the format has 24 characters.
const MAX_NAME: usize = 64;

/// The importer for OFX, with the ID `ofx`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ofx;

impl Importer for Ofx {
    fn info(&self) -> ImporterInfo {
        ImporterInfo {
            id: "ofx",
            kind: ImportKind::Statements,
            extensions: &["ofx", "qfx"],
        }
    }

    fn recognize(&self, start: &[u8]) -> Recognition {
        let found = |needle: &[u8]| find_ignoring_case(start, needle).is_some();
        if found(b"OFXHEADER:") || found(ROOT) || found(b"<?OFX") {
            Recognition::Yes
        } else {
            Recognition::No
        }
    }

    fn read(&self, input: &[u8], limits: &Limits) -> Result<Imported, ImportError> {
        let root =
            find_ignoring_case(input, ROOT).ok_or(ImportError::Malformed { line: Some(1) })?;
        if root > MAX_PREAMBLE {
            return Err(ImportError::Malformed { line: Some(1) });
        }
        let preamble = String::from_utf8_lossy(&input[..root]);
        let encoding = declared_encoding(&preamble)?;
        let text = decode(input, encoding).map_err(|error| match error {
            ImportError::WrongEncoding { line, .. } => ImportError::Malformed { line },
            other => other,
        })?;
        // The root is found again in the decoded text: a byte order mark or
        // a character set may have moved it.
        let start = find_ignoring_case(text.as_bytes(), ROOT)
            .ok_or(ImportError::Malformed { line: Some(1) })?;
        let first_line = line_count(&text.as_bytes()[..start]).saturating_add(1);
        let statements = Parser::new(&text[start..], first_line, limits).run()?;
        Ok(Imported::Statements(statements))
    }
}

/// Where `needle` first occurs in `haystack`, comparing ASCII letters
/// without regard to case.
fn find_ignoring_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

/// How many line breaks `bytes` has.
fn line_count(bytes: &[u8]) -> u64 {
    let breaks = bytes.iter().filter(|&&byte| byte == b'\n').count();
    u64::try_from(breaks).unwrap_or(u64::MAX)
}

/// The text encoding the part of the file before `<OFX>` declares: the
/// `ENCODING` and `CHARSET` lines of an OFX 1.x header, or the `encoding` of
/// an OFX 2.x file's XML declaration. None if it declares none, or `NONE`,
/// which makes the text UTF-8 if it is valid UTF-8 and Windows-1252
/// otherwise. A character set the importer has no table for is refused
/// rather than read as another one, since that would show wrong letters.
fn declared_encoding(preamble: &str) -> Result<Option<TextEncoding>, ImportError> {
    let malformed = |line: usize| ImportError::Malformed {
        line: Some(u64::try_from(line).unwrap_or(u64::MAX).saturating_add(1)),
    };
    let preamble = preamble.strip_prefix('\u{feff}').unwrap_or(preamble);
    let trimmed = preamble.trim();
    // Only processing instructions: a document type definition (or any other
    // `<!` markup) could define entities.
    if let Some(at) = preamble.find("<!") {
        return Err(malformed(preamble[..at].matches('\n').count()));
    }
    if !(trimmed.is_empty()
        || trimmed.starts_with("OFXHEADER:")
        || trimmed.starts_with("<?xml")
        || trimmed.starts_with("<?OFX"))
    {
        return Err(malformed(0));
    }
    let mut encoding = None;
    for (line, text) in preamble.lines().enumerate() {
        let text = text.trim();
        let declared = if let Some(value) = text.strip_prefix("ENCODING:") {
            // `USASCII` and `UNICODE` say nothing the character set doesn't.
            if value.trim().eq_ignore_ascii_case("UTF-8") {
                Some(TextEncoding::Utf8)
            } else {
                continue;
            }
        } else if let Some(value) = text.strip_prefix("CHARSET:") {
            named_encoding(value).map_err(|()| malformed(line))?
        } else if text.starts_with("<?xml") {
            match text.split_once("encoding=") {
                Some((_, rest)) => {
                    let quote = rest.chars().next().filter(|&c| c == '"' || c == '\'');
                    let name = quote
                        .and_then(|quote| rest[1..].split_once(quote))
                        .map(|(name, _)| name)
                        .ok_or_else(|| malformed(line))?;
                    named_encoding(name).map_err(|()| malformed(line))?
                }
                None => None,
            }
        } else {
            None
        };
        if encoding.is_none() {
            encoding = declared;
        }
    }
    Ok(encoding)
}

/// The encoding a header or declaration names: the Windows code page, or
/// the ISO and Unicode names banks use. `NONE` is none; a name the importer
/// has no table for is an error.
fn named_encoding(name: &str) -> Result<Option<TextEncoding>, ()> {
    let name = name.trim();
    let is = |wanted: &str| name.eq_ignore_ascii_case(wanted);
    if is("NONE") || is("") || is("US-ASCII") || is("USASCII") {
        Ok(None)
    } else if is("UTF-8") || is("UTF8") {
        Ok(Some(TextEncoding::Utf8))
    } else if is("1252") || is("windows-1252") || is("cp1252") || is("ISO-8859-1") || is("8859-1") {
        Ok(Some(TextEncoding::Windows1252))
    } else if is("ISO-8859-15") || is("8859-15") {
        Ok(Some(TextEncoding::Iso8859_15))
    } else {
        Err(())
    }
}

/// One piece of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token<'a> {
    /// `<NAME>`, or `<NAME/>` if `empty`.
    Start { name: &'a str, empty: bool },
    /// `</NAME>`.
    End(&'a str),
    /// Text between tags, without the white space around it.
    Text(&'a str),
}

/// A token and the line of the file it starts on.
#[derive(Clone, Copy, Debug)]
struct Spanned<'a> {
    token: Token<'a>,
    line: u64,
}

/// Cuts the text into tags and the text between them.
struct Lexer<'a> {
    text: &'a str,
    position: usize,
    /// The line of `position`.
    line: u64,
}

impl<'a> Lexer<'a> {
    fn malformed(&self) -> ImportError {
        ImportError::Malformed {
            line: Some(self.line),
        }
    }

    /// Moves to `to`, counting the lines passed.
    fn advance(&mut self, to: usize) {
        self.line = self
            .line
            .saturating_add(line_count(&self.text.as_bytes()[self.position..to]));
        self.position = to;
    }

    /// The next token, or none at the end of the file. White space between
    /// tags is skipped, and so are processing instructions (`<?...?>`).
    fn next(&mut self) -> Result<Option<Spanned<'a>>, ImportError> {
        loop {
            let rest = &self.text[self.position..];
            if rest.is_empty() {
                return Ok(None);
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                let chunk = &rest[..end];
                let text = chunk.trim();
                // The line the text itself starts on, not the white space
                // before it.
                let leading = &chunk[..chunk.len() - chunk.trim_start().len()];
                let line = self.line.saturating_add(line_count(leading.as_bytes()));
                self.advance(self.position + end);
                if text.is_empty() {
                    continue;
                }
                return Ok(Some(Spanned {
                    token: Token::Text(text),
                    line,
                }));
            }
            let line = self.line;
            let bytes = rest.as_bytes();
            match bytes.get(1) {
                Some(b'?') => {
                    let end = rest.find("?>").ok_or_else(|| self.malformed())?;
                    self.advance(self.position + end + 2);
                    continue;
                }
                // A document type definition, a comment or CDATA: none of
                // them is in a bank's file.
                Some(b'!') | None => return Err(self.malformed()),
                _ => {}
            }
            let closing = bytes[1] == b'/';
            let name_start = if closing { 2 } else { 1 };
            let name_length = bytes[name_start..]
                .iter()
                .take_while(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                })
                .count();
            if name_length == 0 || name_length > MAX_NAME {
                return Err(self.malformed());
            }
            let name = &rest[name_start..name_start + name_length];
            let after = name_start + name_length;
            let (empty, length) = match &bytes[after..] {
                [b'>', ..] => (false, after + 1),
                [b'/', b'>', ..] if !closing => (true, after + 2),
                _ => return Err(self.malformed()),
            };
            self.advance(self.position + length);
            let token = if closing {
                Token::End(name)
            } else {
                Token::Start { name, empty }
            };
            return Ok(Some(Spanned { token, line }));
        }
    }
}

// The data of the file before it is read as numbers and dates. Amounts and
// dates wait for the end of their statement, since the statement's currency
// comes after the account and may come after the first entry, and each
// keeps the line it was on, for the error report.

/// A text with the line it was on.
#[derive(Clone, Debug)]
struct Raw {
    text: String,
    line: u64,
}

#[derive(Debug, Default)]
struct RawStatement {
    line: u64,
    currency: Option<Raw>,
    bank_id: Option<Raw>,
    account_id: Option<Raw>,
    balance: Option<Raw>,
    balance_date: Option<Raw>,
    entries: Vec<RawEntry>,
}

#[derive(Debug, Default)]
struct RawEntry {
    line: u64,
    amount: Option<Raw>,
    posted: Option<Raw>,
    id: Option<Raw>,
    name: Option<Raw>,
    payee_name: Option<Raw>,
    memo: Option<Raw>,
    account_to: Option<Raw>,
    bank_to: Option<Raw>,
    /// `CURSYM` of the entry's `CURRENCY` or `ORIGCURRENCY`.
    currency: Option<Raw>,
}

/// The elements the reader knows by what they hold. Any other element is
/// `Other`, and everything inside it is skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    /// `OFX`, the root.
    Root,
    /// `BANKMSGSRSV1` or `CREDITCARDMSGSRSV1`, the messages about accounts.
    Messages,
    /// `STMTTRNRS` or `CCSTMTTRNRS`, the response to one request.
    Response,
    /// `STMTRS` or `CCSTMTRS`: one statement.
    Statement,
    /// `BANKACCTFROM` or `CCACCTFROM`: the account.
    Account,
    /// `BANKTRANLIST`: the booked entries.
    Entries,
    /// `STMTTRN`: one entry.
    Entry,
    /// `PAYEE` in an entry.
    Payee,
    /// `BANKACCTTO` or `CCACCTTO` in an entry: the other party's account.
    AccountTo,
    /// `CURRENCY` or `ORIGCURRENCY` in an entry.
    EntryCurrency,
    /// `LEDGERBAL`: the balance the bank reports.
    Balance,
    Other,
}

impl Node {
    /// What the element `name` (in capitals) is inside `parent`. None
    /// `parent` is above the root.
    fn of(parent: Option<Self>, name: &str) -> Self {
        match (parent, name) {
            (None, "OFX") => Self::Root,
            (Some(Self::Root), "BANKMSGSRSV1" | "CREDITCARDMSGSRSV1") => Self::Messages,
            (Some(Self::Messages), "STMTTRNRS" | "CCSTMTTRNRS") => Self::Response,
            (Some(Self::Response), "STMTRS" | "CCSTMTRS") => Self::Statement,
            (Some(Self::Statement), "BANKACCTFROM" | "CCACCTFROM") => Self::Account,
            (Some(Self::Statement), "BANKTRANLIST") => Self::Entries,
            (Some(Self::Statement), "LEDGERBAL") => Self::Balance,
            (Some(Self::Entries), "STMTTRN") => Self::Entry,
            (Some(Self::Entry), "PAYEE") => Self::Payee,
            (Some(Self::Entry), "BANKACCTTO" | "CCACCTTO") => Self::AccountTo,
            (Some(Self::Entry), "CURRENCY" | "ORIGCURRENCY") => Self::EntryCurrency,
            _ => Self::Other,
        }
    }
}

/// An element that is open.
struct Open {
    node: Node,
    /// Its name in capitals, to match its end tag.
    name: String,
}

/// Reads the elements of one file.
struct Parser<'a> {
    lexer: Lexer<'a>,
    limits: &'a Limits,
    /// A token read ahead and given back.
    pending: Option<Spanned<'a>>,
    stack: Vec<Open>,
    statement: Option<RawStatement>,
    entry: Option<RawEntry>,
    entries: usize,
    statements: Vec<Statement>,
    root_closed: bool,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str, first_line: u64, limits: &'a Limits) -> Self {
        Self {
            lexer: Lexer {
                text,
                position: 0,
                line: first_line,
            },
            limits,
            pending: None,
            stack: Vec::new(),
            statement: None,
            entry: None,
            entries: 0,
            statements: Vec::new(),
            root_closed: false,
        }
    }

    fn next_token(&mut self) -> Result<Option<Spanned<'a>>, ImportError> {
        match self.pending.take() {
            Some(token) => Ok(Some(token)),
            None => self.lexer.next(),
        }
    }

    fn run(mut self) -> Result<Vec<Statement>, ImportError> {
        while let Some(Spanned { token, line }) = self.next_token()? {
            match token {
                // Text outside an element that holds a value.
                Token::Text(_) => return Err(ImportError::Malformed { line: Some(line) }),
                Token::End(name) => self.close(name, line)?,
                Token::Start { name, empty: true } => self.nothing(name, line)?,
                Token::Start { name, empty: false } => match self.next_token()? {
                    // An element followed by text holds that text; its end
                    // tag is there in OFX 2.x and left out in 1.x.
                    Some(Spanned {
                        token: Token::Text(value),
                        ..
                    }) => {
                        self.leaf(name, value, line)?;
                        self.skip_end_tag(name)?;
                    }
                    Some(Spanned {
                        token: Token::End(end),
                        ..
                    }) if end.eq_ignore_ascii_case(name) => self.nothing(name, line)?,
                    // An element that holds elements is closed by its own
                    // end tag, so the end tag of another one, right after,
                    // ends an OFX 1.x element left empty (`<MEMO>`).
                    Some(
                        end @ Spanned {
                            token: Token::End(_),
                            ..
                        },
                    ) => {
                        // Unless it is an element of elements: that one
                        // lacks its own end tag.
                        if self.holds_elements(name) {
                            return Err(ImportError::Malformed { line: Some(line) });
                        }
                        self.pending = Some(end);
                        self.leaf(name, "", line)?;
                    }
                    other => {
                        self.pending = other;
                        // In an element that holds only values, an element
                        // with elements of its own would be one the
                        // importer knows; any other is a value left empty.
                        if self.holds_only_values() && !self.holds_elements(name) {
                            self.leaf(name, "", line)?;
                        } else {
                            self.open(name, line)?;
                        }
                    }
                },
            }
        }
        if !self.stack.is_empty() {
            // The file ends inside an element: it is cut off.
            return Err(self.lexer.malformed());
        }
        if self.statements.is_empty() {
            return Err(ImportError::Malformed { line: Some(1) });
        }
        Ok(self.statements)
    }

    /// Takes the end tag of the element `name` that holds a value, if it is
    /// the next token.
    fn skip_end_tag(&mut self, name: &str) -> Result<(), ImportError> {
        match self.next_token()? {
            Some(Spanned {
                token: Token::End(end),
                ..
            }) if end.eq_ignore_ascii_case(name) => {}
            other => self.pending = other,
        }
        Ok(())
    }

    /// An element with nothing in it: an empty value (`<MEMO/>`, or `<MEMO>`
    /// with no end tag in OFX 1.x), or in XML an element of elements that
    /// has none (`<BANKTRANLIST></BANKTRANLIST>`).
    fn nothing(&mut self, name: &str, line: u64) -> Result<(), ImportError> {
        if !self.holds_elements(name) {
            self.leaf(name, "", line)
        } else {
            self.open(name, line)?;
            self.close(name, line)
        }
    }

    /// Whether the element `name` in the open element is one the importer
    /// reads the elements of.
    fn holds_elements(&self, name: &str) -> bool {
        let parent = self.stack.last().map(|open| open.node);
        Node::of(parent, &name.to_ascii_uppercase()) != Node::Other
    }

    /// Whether the open element has values for its content, besides the few
    /// elements of elements the importer knows (`PAYEE` in an entry).
    fn holds_only_values(&self) -> bool {
        matches!(
            self.stack.last().map(|open| open.node),
            Some(
                Node::Account
                    | Node::Entry
                    | Node::Payee
                    | Node::AccountTo
                    | Node::EntryCurrency
                    | Node::Balance
            )
        )
    }

    /// An element that holds elements starts.
    fn open(&mut self, name: &str, line: u64) -> Result<(), ImportError> {
        let malformed = || ImportError::Malformed { line: Some(line) };
        if self.root_closed {
            return Err(malformed());
        }
        let name = name.to_ascii_uppercase();
        let parent = self.stack.last().map(|open| open.node);
        let node = Node::of(parent, &name);
        // The first element is the root, and only one.
        if parent.is_none() && node != Node::Root {
            return Err(malformed());
        }
        // An entry outside the list of entries would be skipped with the
        // elements the importer doesn't read, and its money lost.
        if parent == Some(Node::Statement) && name == "STMTTRN" {
            return Err(malformed());
        }
        self.limits
            .check_depth(self.stack.len().saturating_add(1), Some(line))?;
        match node {
            Node::Statement => {
                self.statement = Some(RawStatement {
                    line,
                    ..RawStatement::default()
                });
            }
            Node::Entry => {
                self.entries = self.entries.saturating_add(1);
                self.limits.check_entries(self.entries)?;
                self.entry = Some(RawEntry {
                    line,
                    ..RawEntry::default()
                });
            }
            _ => {}
        }
        self.stack.push(Open { node, name });
        Ok(())
    }

    /// An element that holds elements ends.
    fn close(&mut self, name: &str, line: u64) -> Result<(), ImportError> {
        let malformed = || ImportError::Malformed { line: Some(line) };
        let open = self.stack.pop().ok_or_else(malformed)?;
        if !open.name.eq_ignore_ascii_case(name) {
            return Err(malformed());
        }
        match open.node {
            Node::Entry => {
                if let (Some(entry), Some(statement)) = (self.entry.take(), self.statement.as_mut())
                {
                    statement.entries.push(entry);
                }
            }
            Node::Statement => {
                if let Some(raw) = self.statement.take() {
                    let statement = statement_from(raw, self.limits)?;
                    self.statements.push(statement);
                }
            }
            _ => {}
        }
        self.root_closed = self.stack.is_empty();
        Ok(())
    }

    /// An element that holds the value `value` (escaped as the file has it),
    /// in whatever element is open.
    fn leaf(&mut self, name: &str, value: &str, line: u64) -> Result<(), ImportError> {
        let malformed = || ImportError::Malformed { line: Some(line) };
        if self.root_closed || self.stack.is_empty() {
            return Err(malformed());
        }
        let name = name.to_ascii_uppercase();
        let parent = self.stack.last().map(|open| open.node);
        // An element that holds elements can't hold a value.
        if self.holds_elements(&name) {
            return Err(malformed());
        }
        let limits = self.limits;
        let keep = |slot: &mut Option<Raw>| -> Result<(), ImportError> {
            if slot.is_some() {
                // The same field twice in one element.
                return Err(malformed());
            }
            limits.check_text(value, Some(line))?;
            *slot = Some(Raw {
                text: unescape(value, line)?,
                line,
            });
            Ok(())
        };
        match (parent, name.as_str()) {
            (Some(Node::Statement), "CURDEF") => keep(&mut self.statement_mut().currency),
            (Some(Node::Account), "BANKID") => keep(&mut self.statement_mut().bank_id),
            (Some(Node::Account), "ACCTID") => keep(&mut self.statement_mut().account_id),
            (Some(Node::Balance), "BALAMT") => keep(&mut self.statement_mut().balance),
            (Some(Node::Balance), "DTASOF") => keep(&mut self.statement_mut().balance_date),
            (Some(Node::Entry), "TRNAMT") => keep(&mut self.entry_mut().amount),
            (Some(Node::Entry), "DTPOSTED") => keep(&mut self.entry_mut().posted),
            (Some(Node::Entry), "FITID") => keep(&mut self.entry_mut().id),
            (Some(Node::Entry), "NAME") => keep(&mut self.entry_mut().name),
            (Some(Node::Entry), "MEMO") => keep(&mut self.entry_mut().memo),
            (Some(Node::Payee), "NAME") => keep(&mut self.entry_mut().payee_name),
            (Some(Node::AccountTo), "ACCTID") => keep(&mut self.entry_mut().account_to),
            (Some(Node::AccountTo), "BANKID") => keep(&mut self.entry_mut().bank_to),
            (Some(Node::EntryCurrency), "CURSYM") => keep(&mut self.entry_mut().currency),
            // A correction replaces or deletes an entry from an earlier
            // file, which only the bank can match to the book's.
            (Some(Node::Entry), "CORRECTFITID") if !value.trim().is_empty() => Err(malformed()),
            _ => Ok(()),
        }
    }

    // Where a field belongs is known from the open elements: an entry's
    // fields only come while an entry is open, and a statement's while a
    // statement is.

    fn statement_mut(&mut self) -> &mut RawStatement {
        self.statement
            .as_mut()
            .expect("a statement is open inside its elements")
    }

    fn entry_mut(&mut self) -> &mut RawEntry {
        self.entry
            .as_mut()
            .expect("an entry is open inside its elements")
    }
}

/// `value` with the entities of the format replaced: the five predefined
/// ones and character references. A `&` that starts none of them stays as
/// it is, since banks often leave it unescaped in a name (`AT&T`) and
/// text is only ever data (D-067).
fn unescape(value: &str, line: u64) -> Result<String, ImportError> {
    // A value that runs over several lines of a file with Windows line
    // endings has one kind of line break, as in the other formats.
    let value = value.replace("\r\n", "\n");
    let value = value.as_str();
    if !value.contains('&') {
        return Ok(value.to_owned());
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        // The name stops at a `;` within the next 10 bytes, so a text of
        // nothing but `&` is read in linear time.
        let entity = rest.as_bytes()[1..]
            .iter()
            .take(11)
            .position(|&byte| byte == b';')
            .map(|end| &rest[1..=end]);
        let replacement = entity.and_then(|name| match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => None,
        });
        if let (Some(entity), Some(character)) = (entity, replacement) {
            out.push(character);
            rest = &rest[entity.len() + 2..];
            continue;
        }
        // `&#65;` or `&#x41;`; with no digits it is not a reference.
        let reference = entity
            .and_then(|name| name.strip_prefix('#'))
            .map(|number| match number.strip_prefix(['x', 'X']) {
                Some(hex) => (number, u32::from_str_radix(hex, 16)),
                None => (number, number.parse::<u32>()),
            })
            .filter(|(number, _)| {
                let digits = number.strip_prefix(['x', 'X']).unwrap_or(number);
                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
        if let Some((number, code)) = reference {
            // A reference to something that is no character, or to NUL, is
            // a damaged file.
            let character = code
                .ok()
                .and_then(char::from_u32)
                .filter(|&character| character != '\0')
                .ok_or(ImportError::Malformed { line: Some(line) })?;
            out.push(character);
            rest = &rest[number.len() + 3..];
            continue;
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The calendar day of an OFX date, `YYYYMMDD` and perhaps a time
/// (`HHMMSS`, then `.XXX`) and a time zone (`[-5:EST]`). The day is the one
/// written, with no conversion between time zones (D-061).
fn parse_date(raw: &Raw) -> Result<Date, ImportError> {
    let bad = || ImportError::BadDate {
        found: raw.text.clone(),
        line: Some(raw.line),
    };
    let bytes = raw.text.as_bytes();
    let digits = |from: usize, count: usize| {
        bytes
            .get(from..from + count)
            .filter(|part| part.iter().all(u8::is_ascii_digit))
    };
    if digits(0, 8).is_none() {
        return Err(bad());
    }
    let mut position = 8;
    // The time: hours, minutes and perhaps seconds, with nothing between.
    let time = bytes[position..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if !matches!(time, 0 | 4 | 6) {
        return Err(bad());
    }
    position += time;
    if time == 6 && bytes.get(position) == Some(&b'.') {
        let fraction = bytes[position + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if fraction == 0 {
            return Err(bad());
        }
        position += 1 + fraction;
    }
    // A time zone, in brackets, that is not read.
    let zone = &bytes[position..];
    if !(zone.is_empty() || (zone[0] == b'[' && zone.last() == Some(&b']'))) {
        return Err(bad());
    }
    let number = |from: usize, count: usize| -> Result<i16, ImportError> {
        let part = digits(from, count).ok_or_else(bad)?;
        part.iter()
            .try_fold(0_i16, |sum, digit| {
                sum.checked_mul(10)?.checked_add(i16::from(digit - b'0'))
            })
            .ok_or_else(bad)
    };
    let year = number(0, 4)?;
    let month = i8::try_from(number(4, 2)?).map_err(|_| bad())?;
    let day = i8::try_from(number(6, 2)?).map_err(|_| bad())?;
    Date::new(year, month, day).map_err(|_| bad())
}

/// An amount as OFX writes it: digits with a decimal point or comma, and an
/// optional sign. Everything else is refused, also a thousands separator
/// beside the decimal mark: an amount is never guessed (AGENTS.md §3.1
/// rule 1).
fn parse_ofx_amount(raw: &Raw, currency: Currency) -> Result<Money, ImportError> {
    let mark = if raw.text.contains(',') {
        DecimalMark::Comma
    } else {
        DecimalMark::Point
    };
    parse_amount(raw.text.trim(), mark, currency).map_err(|error| error.at_line(raw.line))
}

/// A statement from the elements read: its currency and account, its
/// closing balance and its entries.
fn statement_from(raw: RawStatement, limits: &Limits) -> Result<Statement, ImportError> {
    let malformed = |line: u64| ImportError::Malformed { line: Some(line) };
    let code = raw
        .currency
        .as_ref()
        .ok_or_else(|| malformed(raw.line))
        .and_then(currency_code)?;
    let currency = currency(code).map_err(|error| {
        error.at_line(raw.currency.as_ref().map_or(raw.line, |found| found.line))
    })?;

    let account = account_from(&raw);
    for text in [&account.iban, &account.number].into_iter().flatten() {
        limits.check_text(text, Some(raw.line))?;
    }

    let closing_balance = match (&raw.balance, &raw.balance_date) {
        (Some(amount), Some(date)) => Some(Balance {
            amount: parse_ofx_amount(amount, currency)?,
            date: parse_date(date)?,
        }),
        (None, None) => None,
        // A balance needs its amount and its day.
        (Some(only), None) | (None, Some(only)) => return Err(malformed(only.line)),
    };

    let mut entries = Vec::with_capacity(raw.entries.len());
    for entry in &raw.entries {
        entries.push(entry_from(entry, currency, limits)?);
    }
    Ok(Statement {
        account,
        currency,
        opening_balance: None,
        closing_balance,
        entries,
    })
}

/// The currency code a text names.
fn currency_code(raw: &Raw) -> Result<CurrencyCode, ImportError> {
    CurrencyCode::new(raw.text.trim()).map_err(|_| ImportError::Malformed {
        line: Some(raw.line),
    })
}

/// How the statement names its account: the IBAN if `ACCTID` has the shape
/// of one (banks in the euro area often put it there), otherwise the bank
/// code and account number as `BLZ/Konto`, or the account number alone.
fn account_from(raw: &RawStatement) -> StatementAccount {
    let id = raw.account_id.as_ref().and_then(|raw| some_text(&raw.text));
    let bank = raw.bank_id.as_ref().and_then(|raw| some_text(&raw.text));
    match id {
        Some(id) if looks_like_iban(&id) => StatementAccount {
            iban: Some(id),
            number: None,
        },
        Some(id) => StatementAccount {
            iban: None,
            number: Some(match bank {
                Some(bank) => format!("{bank}/{id}"),
                None => id,
            }),
        },
        None => StatementAccount::default(),
    }
}

/// An entry from the elements read. Every `STMTTRN` is booked: pending
/// entries come in a list of their own, which is not read.
fn entry_from(raw: &RawEntry, currency: Currency, limits: &Limits) -> Result<Entry, ImportError> {
    let malformed = ImportError::Malformed {
        line: Some(raw.line),
    };
    let amount = raw.amount.as_ref().ok_or_else(|| malformed.clone())?;
    let posted = raw.posted.as_ref().ok_or(malformed)?;
    // `CURRENCY` and `ORIGCURRENCY` are both about another currency than the
    // account's; which of the two the amount is in depends on the file, so
    // an entry in another currency is refused, not guessed.
    if let Some(found) = &raw.currency {
        let found_code = currency_code(found)?;
        if found_code != currency.code() {
            return Err(ImportError::CurrencyMismatch {
                expected: currency.code(),
                found: found_code,
            });
        }
    }
    let amount = parse_ofx_amount(amount, currency)?;
    let text = |raw: &Option<Raw>| raw.as_ref().and_then(|raw| some_text(&raw.text));
    let bank_id = text(&raw.id).filter(|id| {
        !NO_REFERENCE
            .iter()
            .any(|none| id.eq_ignore_ascii_case(none))
    });
    let other_party_iban = match (text(&raw.account_to), text(&raw.bank_to)) {
        (Some(account), _) if looks_like_iban(&account) => Some(account),
        (Some(account), Some(bank)) => Some(format!("{bank}/{account}")),
        (account, _) => account,
    };
    let entry = Entry {
        bank_id,
        value_date: None,
        other_party_name: text(&raw.name).or_else(|| text(&raw.payee_name)),
        other_party_iban,
        remittance_information: text(&raw.memo),
        ..Entry::booked(parse_date(posted)?, amount)
    };
    for text in entry.texts() {
        limits.check_text(text, Some(raw.line))?;
    }
    Ok(entry)
}

#[cfg(test)]
mod tests;
