// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Opening a page of the app for the user (D-265, threat T-36).
//!
//! A model may ask KassenSturz to show the user a page: "where is my
//! streaming Rule?" can end with the Rules page open and that Rule selected.
//! The call only names a page from a fixed list, and an account or a Rule by
//! the ID a read function gave. It reads nothing from the book beyond whether
//! that ID exists, changes nothing, and opens nothing outside the app: the
//! front end opens the page when the model's answer is ready, and says so in
//! the chat. Settings, the bank pages and the pages that change something are
//! not on the list.

use kassensturz_core::ai_switch::{Permissions, Row};
use kassensturz_core::{AccountId, RuleId};
use kassensturz_store::{Book, StoreError};
use serde_json::{Value, json};

use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};

/// The function that opens a page for the user.
pub const OPEN_PAGE: Function = Function {
    name: "open_page",
    description: "Opens a page of KassenSturz for the user, when they ask you to show them \
        something, e.g. \"show me my streaming Rule\". It only shows the page: nothing is \
        changed. \"overview\" is the first screen, \"accounts\" the list of accounts, \
        \"categories\" the categories and \"rules\" the Rules. \"account\" shows the \
        transactions of one account and \"rule\" the Rules page with one Rule selected; both \
        need \"id\", copied from list_accounts or list_rules. The page opens when you have \
        finished your answer, and only the last page you asked for. Say in your answer which \
        page you opened, in the user's words.",
    parameters: r#"{"type":"object","properties":{"page":{"type":"string","enum":["overview","accounts","categories","rules","account","rule"],"description":"Which page to open."},"id":{"type":"string","description":"The ID of the account or Rule, from list_accounts or list_rules. Only for \"account\" and \"rule\"."}},"required":["page"],"additionalProperties":false}"#,
};

/// The arguments of the call, by their fixed names: keywords of
/// `ai-tools.bad-argument`.
#[cfg(test)]
pub(crate) const FIELDS: &[&str] = &["page", "id"];

/// A page of the app the user is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageTarget {
    /// The first screen.
    Overview,
    /// The list of accounts.
    Accounts,
    /// The categories.
    Categories,
    /// The Rules, with the Rule `select` chosen when there is one.
    Rules {
        /// The Rule's ID as the read functions write it.
        select: Option<String>,
    },
    /// The transactions of one account.
    Account {
        /// The account's ID as the read functions write it.
        id: String,
    },
}

impl PageTarget {
    /// The page's fixed name, as the model writes it and the front end reads
    /// it; `rules` also stands for one Rule selected.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Accounts => "accounts",
            Self::Categories => "categories",
            Self::Rules { .. } => "rules",
            Self::Account { .. } => "account",
        }
    }

    /// The ID of the account or Rule the page is about, if it is about one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Rules { select } => select.as_deref(),
            Self::Account { id } => Some(id),
            _ => None,
        }
    }
}

/// Checks a model's call of [`OPEN_PAGE`] with `arguments` against `book`
/// and gives the page to open and the result the model gets. Nothing is
/// opened here.
pub fn open_page(book: &Book, arguments: &Value) -> Result<(PageTarget, Output), ToolError> {
    let bad = |argument| ToolError::BadArgument { argument };
    let Value::Object(values) = arguments else {
        return Err(bad("arguments"));
    };
    if values.keys().any(|key| key != "page" && key != "id") {
        return Err(bad("arguments"));
    }
    let text = |name: &'static str| match values.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(bad(name)),
    };
    let page = text("page")?.ok_or(bad("page"))?;
    let id = text("id")?;
    let target = match (page, id) {
        ("overview", None) => PageTarget::Overview,
        ("accounts", None) => PageTarget::Accounts,
        ("categories", None) => PageTarget::Categories,
        ("rules", None) => PageTarget::Rules { select: None },
        ("overview" | "accounts" | "categories" | "rules", Some(_)) => return Err(bad("id")),
        ("account", Some(text)) => {
            let id = uuid(text).map(AccountId::from_bytes).ok_or(bad("id"))?;
            match book.account(id) {
                Ok(account) if account.kind.is_account() => {}
                Ok(_) | Err(StoreError::UnknownAccount(_)) => {
                    return Err(ToolError::UnknownAccount);
                }
                Err(error) => return Err(error.into()),
            }
            PageTarget::Account { id: id.to_string() }
        }
        ("rule", Some(text)) => {
            let id = uuid(text).map(RuleId::from_bytes).ok_or(bad("id"))?;
            match book.rule(id) {
                Ok(_) => {}
                Err(StoreError::UnknownRule(_)) => return Err(ToolError::UnknownRule),
                Err(error) => return Err(error.into()),
            }
            PageTarget::Rules {
                select: Some(id.to_string()),
            }
        }
        ("account" | "rule", None) => return Err(bad("id")),
        _ => return Err(bad("page")),
    };
    let output = Output::new(
        json!({
            "opened": target.name(),
            "message": "KassenSturz will show this page to the user when you have finished \
                        your answer. Nothing was changed.",
        }),
        Figures::new(),
    );
    Ok((target, output))
}

/// [`open_page`] for a page whose list the user allows the AI to read
/// (D-267, D-252): `rules` and `rule` need the reading of Rules, `accounts`
/// and `account` of Accounts, `categories` of Categories; `overview` shows no
/// list. A page that isn't allowed is refused (`ai-tools.not-allowed`, argument
/// `page`) before its ID is looked up, so the answer doesn't tell whether the
/// ID exists.
pub fn open_page_allowed(
    book: &Book,
    arguments: &Value,
    permissions: Permissions,
) -> Result<(PageTarget, Output), ToolError> {
    let row = match arguments.get("page").and_then(Value::as_str) {
        Some("rules" | "rule") => Some(Row::Rules),
        Some("accounts" | "account") => Some(Row::Accounts),
        Some("categories") => Some(Row::Categories),
        _ => None,
    };
    if row.is_some_and(|row| !permissions.reads(row)) {
        return Err(ToolError::NotAllowed { argument: "page" });
    }
    open_page(book, arguments)
}

/// The bytes of an ID written in the usual hyphenated form.
fn uuid(text: &str) -> Option<[u8; 16]> {
    uuid::Uuid::try_parse(text).ok().map(uuid::Uuid::into_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_names_only_the_fixed_arguments_and_the_pages_match() {
        let schema: Value = serde_json::from_str(OPEN_PAGE.parameters).expect("JSON");
        assert_eq!(schema["additionalProperties"], false);
        let properties = schema["properties"].as_object().expect("properties");
        let mut keys: Vec<&str> = properties.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut fields = FIELDS.to_vec();
        fields.sort_unstable();
        assert_eq!(keys, fields);
        let pages: Vec<&str> = properties["page"]["enum"]
            .as_array()
            .expect("a list of pages")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let named = [
            PageTarget::Overview,
            PageTarget::Accounts,
            PageTarget::Categories,
            PageTarget::Rules { select: None },
            PageTarget::Account { id: String::new() },
        ];
        for target in &named {
            assert!(pages.contains(&target.name()), "{}", target.name());
        }
        // "rule" is the one page name that opens the Rules page.
        assert_eq!(pages.len(), named.len() + 1);
    }

    #[test]
    fn it_is_not_one_of_the_read_functions() {
        // The read-only MCP server offers FUNCTIONS and nothing else (D-265).
        assert!(crate::FUNCTIONS.iter().all(|f| f.name != OPEN_PAGE.name));
    }
}
