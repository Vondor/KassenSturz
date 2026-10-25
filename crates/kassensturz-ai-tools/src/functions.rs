// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fixed list of functions a model may call, and how a call reaches
//! them (D-170, D-186, threat T-23).
//!
//! The list is a constant reviewed with the threat model: a function is
//! added only with its row in `docs/THREAT_MODEL.md` (T-23), which a test
//! checks. A call names a function and passes JSON arguments; a name not in
//! the list, an argument the function doesn't take or a value of the wrong
//! kind is refused with an error code, never guessed at.

use kassensturz_core::{AccountId, Date};
use kassensturz_store::Book;
use serde_json::{Map, Value};

use crate::error::ToolError;
use crate::figures::Figures;
use crate::read::{self, Names, Search};
use crate::text::DATA_NOTE;
use crate::writer::Writer;

/// A function a model may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Function {
    /// Its name, as the model calls it.
    pub name: &'static str,
    /// What it does, for the model. English, like the rest of what
    /// KassenSturz tells a model; it is never shown to the user (D-189).
    pub description: &'static str,
    /// Its arguments as a JSON Schema object, the form both function
    /// calling (D-035) and MCP (D-050) use.
    pub parameters: &'static str,
}

/// Every function a model may call, in the order they are offered.
pub const FUNCTIONS: &[Function] = &[
    Function {
        name: "list_accounts",
        description: "Lists the user's accounts (not categories) with each one's current \
            balance and ID, and what the balances add up to per currency. A debt such as a \
            credit card has \"debt\": true and a negative balance for what is owed. Amounts \
            are exact decimal text with a dot as the decimal mark.",
        parameters: r#"{"type":"object","properties":{"include_closed":{"type":"boolean","description":"Also list closed accounts. Default false."}},"additionalProperties":false}"#,
    },
    Function {
        name: "account_balance",
        description: "The balance of one of the user's accounts at the end of a day, counting \
            each entry on the day the bank booked it.",
        parameters: r#"{"type":"object","properties":{"account_id":{"type":"string","description":"The account's ID from list_accounts."},"date":{"type":"string","format":"date","description":"YYYY-MM-DD. Default today."}},"required":["account_id"],"additionalProperties":false}"#,
    },
    Function {
        name: "category_totals",
        description: "Money in and money out by category for a period, per currency, largest \
            first: income categories with what came in, spending categories with what went \
            out, \"left\" as money in minus money out, and how many transactions moved money \
            between the user's own accounts. Such moves and starting balances count as \
            neither income nor spending. Money not sorted into a category yet has \
            \"unsorted\": true.",
        parameters: r#"{"type":"object","properties":{"from":{"type":"string","format":"date","description":"First day, YYYY-MM-DD."},"to":{"type":"string","format":"date","description":"Last day, YYYY-MM-DD."}},"required":["from","to"],"additionalProperties":false}"#,
    },
    Function {
        name: "search_transactions",
        description: "Finds entries in the user's accounts, newest first, one row per account \
            an entry is in, with the amount as seen from that account (money in positive, \
            money out negative), the payee, memo and bank text, and the category, the other \
            account of a transfer, or \"unsorted\": true. Returns how many matched in all and \
            at most \"limit\" rows. All filters are optional and combine. With \
            \"unsorted_only\" only entries not sorted into a category yet are found.",
        parameters: r#"{"type":"object","properties":{"text":{"type":"string","maxLength":100,"description":"Text to find in the payee, memo or bank text, ignoring case."},"account_id":{"type":"string","description":"Only entries of this account, ID from list_accounts."},"category_id":{"type":"string","description":"Only transactions booked to this category or one below it, ID from list_categories."},"from":{"type":"string","format":"date","description":"First booking day, YYYY-MM-DD."},"to":{"type":"string","format":"date","description":"Last booking day, YYYY-MM-DD."},"limit":{"type":"integer","minimum":1,"maximum":50,"description":"Most rows to return. Default 20."},"unsorted_only":{"type":"boolean","description":"Only entries that have no category yet. Default false."}},"additionalProperties":false}"#,
    },
    Function {
        name: "list_categories",
        description: "Lists the income and spending categories with their IDs, each with the \
            categories above it in its name, e.g. \"Food › Groceries\".",
        parameters: r#"{"type":"object","properties":{},"additionalProperties":false}"#,
    },
    Function {
        name: "list_rules",
        description: "Lists the user's Rules, which sort new transactions into categories: \
            what each one looks for, what it books to, and its sub-Rules.",
        parameters: r#"{"type":"object","properties":{},"additionalProperties":false}"#,
    },
];

/// The name of every argument any function takes, and `arguments` for a
/// call whose arguments as a whole are wrong. The keywords of
/// `ai-tools.bad-argument` (`docs/ERROR_CODES.md`), which the tests check
/// against the schemas above.
#[cfg(test)]
pub(crate) const ARGUMENTS: &[&str] = &[
    "arguments",
    "include_closed",
    "account_id",
    "date",
    "from",
    "to",
    "text",
    "category_id",
    "limit",
    "unsorted_only",
];

/// The most characters of the text a search looks for.
const MAX_SEARCH_TEXT: usize = 100;

/// What a function returned: the JSON the model gets, and the figures in
/// it for the number check ([`crate::check_numbers`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    /// The result: an object whose `note` says that its
    /// text is data ([`DATA_NOTE`], D-171).
    pub value: Value,
    /// Every amount, count and number in a text that `value` holds.
    pub figures: Figures,
}

impl Output {
    pub(crate) fn new(result: Value, figures: Figures) -> Self {
        let mut value = Map::new();
        value.insert("note".into(), Value::from(DATA_NOTE));
        if let Value::Object(result) = result {
            value.extend(result);
        }
        Self {
            value: Value::Object(value),
            figures,
        }
    }
}

/// Calls the function `name` of [`FUNCTIONS`] on `book` with `arguments`,
/// as a model sent them: a JSON object, or null for none. `today` is the
/// day of this computer's clock, for defaults such as a balance today.
pub fn call(book: &Book, name: &str, arguments: &Value, today: Date) -> Result<Output, ToolError> {
    let function = FUNCTIONS
        .iter()
        .find(|function| function.name == name)
        .ok_or(ToolError::UnknownFunction)?;
    let args = Arguments::read(arguments, function)?;
    let mut w = Writer::new(book);
    let result = match function.name {
        "list_accounts" => read::list_accounts(
            book,
            &mut w,
            args.boolean("include_closed")?.unwrap_or(false),
        )?,
        "account_balance" => {
            let names = Names::read(book)?;
            let account = args.id("account_id")?.ok_or(ToolError::BadArgument {
                argument: "account_id",
            })?;
            let date = args.date("date")?.unwrap_or(today);
            read::account_balance(book, &mut w, &names, account, date)?
        }
        "category_totals" => {
            let names = Names::read(book)?;
            let from = args
                .date("from")?
                .ok_or(ToolError::BadArgument { argument: "from" })?;
            let to = args
                .date("to")?
                .ok_or(ToolError::BadArgument { argument: "to" })?;
            if to < from {
                return Err(ToolError::BadPeriod);
            }
            read::category_totals(book, &mut w, &names, from, to, read::MAX_TOTALS)?
        }
        "search_transactions" => {
            let names = Names::read(book)?;
            let search = Search {
                text: args.search_text()?,
                account: args.id("account_id")?,
                category: args.id("category_id")?,
                from: args.date("from")?,
                to: args.date("to")?,
                limit: args.limit()?,
                unsorted_only: args.boolean("unsorted_only")?.unwrap_or(false),
            };
            if let (Some(from), Some(to)) = (search.from, search.to)
                && to < from
            {
                return Err(ToolError::BadPeriod);
            }
            read::search_transactions(book, &mut w, &names, &search)?
        }
        "list_categories" => read::list_categories(book, &mut w)?,
        "list_rules" => read::list_rules(book, &mut w, &Names::read(book)?)?,
        _ => return Err(ToolError::UnknownFunction),
    };
    Ok(Output::new(result, w.figures))
}

/// The small overview for a model that can't call functions (D-052): the
/// open accounts with their balances and what they add up to, and this
/// month's and last month's money in and out by category, the largest few
/// per side. A question beyond it is one the model must say it can't
/// answer exactly; M5.4's instructions tell it so.
pub fn overview(book: &Book, today: Date) -> Result<Output, ToolError> {
    let names = Names::read(book)?;
    let mut w = Writer::new(book);
    let first = today.first_of_month();
    let mut result = Map::new();
    result.insert("today".into(), Value::String(today.to_string()));
    if let Value::Object(accounts) = read::list_accounts(book, &mut w, false)? {
        result.extend(accounts);
    }
    let this_month = read::category_totals(
        book,
        &mut w,
        &names,
        first,
        today.last_of_month(),
        read::OVERVIEW_TOTALS,
    )?;
    result.insert("this_month".into(), this_month);
    if let Ok(last) = first.yesterday() {
        let last_month = read::category_totals(
            book,
            &mut w,
            &names,
            last.first_of_month(),
            last,
            read::OVERVIEW_TOTALS,
        )?;
        result.insert("last_month".into(), last_month);
    }
    Ok(Output::new(Value::Object(result), w.figures))
}

/// The arguments of one call, checked against what its function takes.
struct Arguments<'a> {
    values: Option<&'a Map<String, Value>>,
}

impl<'a> Arguments<'a> {
    fn read(arguments: &'a Value, function: &Function) -> Result<Self, ToolError> {
        let bad = ToolError::BadArgument {
            argument: "arguments",
        };
        let values = match arguments {
            Value::Null => None,
            Value::Object(values) => Some(values),
            _ => return Err(bad),
        };
        let schema: Value =
            serde_json::from_str(function.parameters).map_err(|_| ToolError::BadArgument {
                argument: "arguments",
            })?;
        let known = schema.get("properties").and_then(Value::as_object);
        if let Some(values) = values
            && values
                .keys()
                .any(|key| !known.is_some_and(|known| known.contains_key(key)))
        {
            return Err(bad);
        }
        Ok(Self { values })
    }

    /// The argument `name`, if given; null counts as not given.
    fn get(&self, name: &str) -> Option<&'a Value> {
        self.values
            .and_then(|values| values.get(name))
            .filter(|value| !value.is_null())
    }

    fn string(&self, name: &'static str) -> Result<Option<&'a str>, ToolError> {
        self.get(name)
            .map(|value| {
                value
                    .as_str()
                    .ok_or(ToolError::BadArgument { argument: name })
            })
            .transpose()
    }

    fn boolean(&self, name: &'static str) -> Result<Option<bool>, ToolError> {
        self.get(name)
            .map(|value| {
                value
                    .as_bool()
                    .ok_or(ToolError::BadArgument { argument: name })
            })
            .transpose()
    }

    /// A day written `YYYY-MM-DD`, nothing else.
    fn date(&self, name: &'static str) -> Result<Option<Date>, ToolError> {
        self.string(name)?
            .map(|text| {
                let wellformed = text.len() == 10
                    && text.char_indices().all(|(at, c)| match at {
                        4 | 7 => c == '-',
                        _ => c.is_ascii_digit(),
                    });
                wellformed
                    .then(|| text.parse::<Date>().ok())
                    .flatten()
                    .ok_or(ToolError::BadArgument { argument: name })
            })
            .transpose()
    }

    /// An account's or category's ID, as the functions write it.
    fn id(&self, name: &'static str) -> Result<Option<AccountId>, ToolError> {
        self.string(name)?
            .map(|text| {
                uuid::Uuid::try_parse(text)
                    .map(|id| AccountId::from_bytes(id.into_bytes()))
                    .map_err(|_| ToolError::BadArgument { argument: name })
            })
            .transpose()
    }

    fn search_text(&self) -> Result<Option<String>, ToolError> {
        let Some(text) = self.string("text")? else {
            return Ok(None);
        };
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_SEARCH_TEXT {
            return Err(ToolError::BadArgument { argument: "text" });
        }
        Ok(Some(text.to_lowercase()))
    }

    fn limit(&self) -> Result<usize, ToolError> {
        let bad = ToolError::BadArgument { argument: "limit" };
        match self.get("limit") {
            None => Ok(read::DEFAULT_SEARCH_ROWS),
            Some(value) => value
                .as_u64()
                .and_then(|limit| usize::try_from(limit).ok())
                .filter(|limit| (1..=read::MAX_SEARCH_ROWS).contains(limit))
                .ok_or(bad),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_has_a_valid_schema_naming_only_known_arguments() {
        for function in FUNCTIONS {
            let schema: Value =
                serde_json::from_str(function.parameters).expect("the schema is JSON");
            assert_eq!(schema["type"], "object", "{}", function.name);
            assert_eq!(schema["additionalProperties"], false, "{}", function.name);
            for argument in schema["properties"].as_object().expect("properties").keys() {
                assert!(
                    ARGUMENTS.contains(&argument.as_str()),
                    "{}: {argument} isn't in ARGUMENTS",
                    function.name
                );
            }
        }
    }

    #[test]
    fn every_function_is_in_the_threat_model() {
        // A function is added only with its row in the threat model (T-23).
        let threats = include_str!("../../../docs/THREAT_MODEL.md");
        let row = threats
            .lines()
            .find(|line| line.starts_with("| T-23 |"))
            .expect("the threat model has T-23");
        for function in FUNCTIONS {
            assert!(
                row.contains(&format!("`{}`", function.name)),
                "{} isn't named in T-23",
                function.name
            );
        }
    }

    #[test]
    fn names_are_unique_and_plain() {
        for (index, function) in FUNCTIONS.iter().enumerate() {
            assert!(
                function
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                function.name
            );
            assert!(
                FUNCTIONS[index + 1..]
                    .iter()
                    .all(|other| other.name != function.name)
            );
        }
    }
}
