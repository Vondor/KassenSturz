// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A name next to an ID (US2.18, #598, D-675): the IDs are long and share
//! most of their characters, so a small model may copy one wrongly. An item
//! of a call may carry the name it means next to the ID (`category_name`
//! next to `category_id`, and so on), and the call is refused when the two
//! don't fit, so a miscopied ID that happens to name another item is caught
//! too. The names are taken out of the call before it is checked further,
//! so what the user edits and what is applied never holds them.

use std::collections::HashMap;

use kassensturz_core::{AccountId, RuleId};
use kassensturz_store::Book;
use serde_json::{Map, Value};

use super::SECTIONS;
use crate::error::ToolError;
use crate::text::for_model;

/// Each ID that may carry a name, the name's key, and what it names.
pub(crate) const NAMED_IDS: [(&str, &str, Kind); 5] = [
    ("category_id", "category_name", Kind::Category),
    ("parent_id", "parent_name", Kind::Category),
    ("account_id", "account_name", Kind::Account),
    ("rule_id", "rule_name", Kind::Rule),
    ("parent_rule_id", "parent_rule_name", Kind::Rule),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Category,
    Account,
    Rule,
}

/// The names of the book as the read functions show them, ignoring
/// capitals: a category by its path (`Food › Groceries`) or its own name.
struct Names {
    categories: HashMap<AccountId, Vec<String>>,
    accounts: HashMap<AccountId, String>,
    rules: HashMap<RuleId, String>,
}

impl Names {
    fn read(book: &Book) -> Result<Self, ToolError> {
        let categories = book
            .categories()?
            .into_iter()
            .map(|entry| (entry.category.id, entry.path))
            .collect();
        let accounts = book
            .all_accounts()?
            .into_iter()
            .map(|account| (account.id, account.name))
            .collect();
        let mut rules = HashMap::new();
        let mut nodes = book.rules()?;
        while let Some(node) = nodes.pop() {
            rules.insert(node.rule.id, node.rule.name.clone());
            nodes.extend(node.sub_rules);
        }
        Ok(Self {
            categories,
            accounts,
            rules,
        })
    }

    /// Whether `given` fits the item `id` of `kind`. `None` when the ID
    /// names nothing of the book: the check that follows refuses it.
    fn fits(&self, kind: Kind, id: [u8; 16], given: &str) -> Option<bool> {
        let given = plain(given);
        match kind {
            Kind::Category => {
                let path = self.categories.get(&AccountId::from_bytes(id))?;
                let full = path.join(" › ");
                Some(
                    given == plain(&full)
                        || path.last().is_some_and(|own| given == plain(own))
                        || given == plain(&path.join(" > ")),
                )
            }
            Kind::Account => {
                let name = self.accounts.get(&AccountId::from_bytes(id))?;
                Some(given == plain(name))
            }
            Kind::Rule => {
                let name = self.rules.get(&RuleId::from_bytes(id))?;
                Some(given == plain(name))
            }
        }
    }
}

/// A name as a model sees it, trimmed and without capitals.
fn plain(text: &str) -> String {
    for_model(text).trim().to_lowercase()
}

/// Checks the names next to the IDs in the items of `arguments` and
/// returns the call without them. A name that isn't text, or comes without
/// its ID, is `ai-tools.bad-argument`; one that doesn't fit its ID is
/// `ai-tools.name-mismatch` with the ID's key, in the item it is in.
pub(super) fn take_names(book: &Book, arguments: &Value) -> Result<Value, ToolError> {
    let mut call = arguments.clone();
    let Some(object) = call.as_object_mut() else {
        return Ok(call);
    };
    if !object.values().any(has_name) {
        return Ok(call);
    }
    let names = Names::read(book)?;
    for (list, _) in SECTIONS {
        let Some(Value::Array(items)) = object.get_mut(list) else {
            continue;
        };
        for (index, item) in items.iter_mut().enumerate() {
            let Some(item) = item.as_object_mut() else {
                continue;
            };
            for (id_key, name_key, kind) in NAMED_IDS {
                let Some(name) = item.remove(name_key) else {
                    continue;
                };
                let bad = || ToolError::BadArgument { argument: name_key }.at(list, index);
                let Value::String(name) = name else {
                    return Err(bad());
                };
                let id = item
                    .get(id_key)
                    .ok_or_else(bad)?
                    .as_str()
                    .and_then(|text| uuid::Uuid::try_parse(text).ok());
                // A malformed or unknown ID is refused by the check that
                // follows, with its own code.
                if let Some(id) = id
                    && names.fits(kind, id.into_bytes(), &name) == Some(false)
                {
                    return Err(ToolError::NameMismatch { argument: id_key }.at(list, index));
                }
            }
        }
    }
    Ok(call)
}

/// Whether a list holds an item with a name next to an ID.
fn has_name(list: &Value) -> bool {
    list.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            NAMED_IDS
                .iter()
                .any(|(_, name_key, _)| item.get(name_key).is_some())
        })
    })
}

/// Takes the name of an ID out of an item's offered fields when the ID
/// itself is not offered, such as a locked account (D-257).
pub(super) fn without_orphans(fields: &mut Map<String, Value>) {
    for (id, name, _) in NAMED_IDS {
        if !fields.contains_key(id) {
            fields.remove(name);
        }
    }
}
