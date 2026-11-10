// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Change sets: what a model wants to change in a book, held for the user
//! to review, and applied only when the user confirms (D-171, D-199 to
//! D-201, AGENTS.md §3.1 rule 9, §8).
//!
//! A model prepares one with [`PROPOSE_CHANGES`]: new categories, renaming
//! or moving categories, new Rules, editing, moving and deleting Rules, and
//! a category for transactions, nothing else (D-236). [`propose`] checks the
//! call against the book and turns it into a [`ChangeSet`] whose
//! [`ProposedChange`]s are what the user sees, before and after, with exact
//! amounts. Nothing in the book changes then. Only [`ChangeSet::apply`]
//! changes it, and it takes a [`Confirmed`], which only the user's confirm
//! button makes. It runs the store's own operations as one user action,
//! [`Action::ApplyAiChanges`] (D-198), so one undo takes back the whole set
//! and the history keeps it apart from the user's own changes.
//!
//! The set doesn't care how the call arrived: the chat hands it over in
//! memory, and the MCP write tools (M5.10) will hand over the same JSON
//! through a request file. Whatever the way, the app checks and shows it
//! here, and applies it here.

use std::collections::{HashMap, HashSet};

mod accounts;
mod budgets;
mod cash;
mod edits;
mod transactions;
mod view_call;
mod views;

use kassensturz_core::ai_switch::{Field, Permission, Permissions};
use kassensturz_core::{
    Account, AccountId, AccountKind, Actions, AmountTest, BookTo, Condition, CurrencyCode, Date,
    ErrorReport, MatchMode, Money, Report, Rule, RuleError, RuleId, RuleNode, Split, SplitId,
    TextField, TextOperator, Transaction, TransactionId,
};
use kassensturz_store::{Action, Book, StoreError};
use serde_json::{Map, Value, json};

use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};
use crate::read::MAX_RULE_DEPTH;
use crate::text::{for_model, hides_text};

pub use accounts::{DeleteCategory, EditAccount, NewAccount};
pub use budgets::{AmountBasis, EditBudget, NewBudget, every_word};
pub use cash::{DeleteTransaction, NewTransaction};
pub use edits::{EditError, Slot};
pub use transactions::{EditTransaction, FieldEdit, FieldValue};
#[cfg(test)]
pub(crate) use view_call::ARGUMENTS as VIEW_ARGUMENTS;
pub(crate) use view_call::list_views;
pub use view_call::{
    MAX_VIEW_ACCOUNTS, MAX_VIEW_CALL_BYTES, MAX_VIEW_CATEGORIES, MAX_VIEWS_PER_ANSWER,
    PROPOSE_VIEW_NAME, propose_view, propose_view_spec, propose_view_text, view_choices,
};
#[cfg(test)]
use view_call::{default_period, fits, formula_help};
pub use views::{DeleteView, EditView, NewView};

/// The function a model calls to prepare changes. It is not in
/// [`crate::FUNCTIONS`], the read functions every way in offers: the chat
/// offers it next to them, and the MCP server only once its write tools are
/// turned on (M5.10, D-035).
pub const PROPOSE_CHANGES: Function = Function {
    name: "propose_changes",
    description: "Prepares changes to the user's book for the user to review: new \
        categories, renaming or moving categories, new Rules that sort future transactions \
        into a category, changing, moving or deleting existing Rules, and a category for \
        transactions you found with search_transactions, deleting empty categories, \
        adding, renaming, closing, reopening or changing the type of accounts and preparing or changing budgets. A \
        chart is prepared with propose_view instead (only when the user allows it). Nothing changes until the user confirms them in KassenSturz's window, and the user may refuse them. A new category \
        has a key of your choice, which edit_categories (as new_parent), new_rules, \
        edit_rules and categorize use as new_category. Only a transaction with one category, \
        or none yet, can get another one; a transfer between the user's own accounts can't. \
        Rules are checked in order and the first that matches wins, so a new Rule goes last \
        until you move it with move_rules. A Rule's name is used once among Rules next to each \
        other, ignoring capitals; to replace a Rule by a new one of the same name, delete the \
        old one in the same call. One call can both change and move a Rule, but what it \
        deletes it can't also change or move. A Rule can sit inside another (parent_rule_id, or new_parent_rule \
        with the key of a Rule earlier in new_rules): a sub-Rule is checked only when its \
        parent matched, and one Rule with a pattern or many conditions is better than a \
        second Rule with a number in its name. Categories and Rules are named by the IDs that \
        list_categories and list_rules give. Prepare everything that belongs together in one \
        call.",
    parameters: r#"{"type":"object","properties":{"new_categories":{"type":"array","maxItems":20,"items":{"type":"object","properties":{"key":{"type":"string","maxLength":40,"description":"Your short name for it, used as new_category below."},"name":{"type":"string","maxLength":100},"parent_id":{"type":"string","description":"Put it inside this category, ID from list_categories. Default: at the top."},"income":{"type":"boolean","description":"An income category, not a spending one. Default false. Inside a category it must be the same kind."},"currency":{"type":"string","description":"ISO 4217 code. Default: the parent's, or the one all the user's accounts use."}},"required":["key","name"],"additionalProperties":false}},"new_rules":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"key":{"type":"string","maxLength":40,"description":"Your short name for the Rule, so that a later Rule of this list can sit inside it with new_parent_rule."},"name":{"type":"string","maxLength":100},"parent_rule_id":{"type":"string","description":"Make it a sub-Rule of this Rule, ID from list_rules. A sub-Rule is checked only when its parent matched; the deepest one that matches decides. Default: a top-level Rule."},"new_parent_rule":{"type":"string","description":"Or the key of a Rule earlier in this list."},"match":{"type":"string","enum":["all","any"],"description":"Whether every condition must hold or one is enough. Default all."},"conditions":{"type":"array","minItems":1,"maxItems":100,"items":{"type":"object","description":"Either field, test and text, or money_in: true, or money_out: true.","properties":{"field":{"type":"string","enum":["payee","other_party_name","bank_text"]},"test":{"type":"string","enum":["contains","equals","starts_with","ends_with","matches_pattern"],"description":"matches_pattern: text is a pattern in the syntax of Rust's regex crate (for example \"aldi|lidl|rewe\"); lookahead, lookbehind and backreferences are not supported. One pattern can replace many contains conditions."},"text":{"type":"string","maxLength":1000,"description":"At most 100 characters, but a matches_pattern pattern may have up to 1000."},"case_sensitive":{"type":"boolean","description":"Default false: capitals are ignored, also in a pattern."},"money_in":{"type":"boolean"},"money_out":{"type":"boolean"}},"additionalProperties":false}},"category_id":{"type":"string","description":"Where it books what it finds, ID from list_categories."},"new_category":{"type":"string","description":"Or the key of a category in new_categories."}},"required":["name","conditions"],"additionalProperties":false}},"edit_categories":{"type":"array","maxItems":20,"items":{"type":"object","properties":{"category_id":{"type":"string","description":"The category to rename or move, ID from list_categories."},"name":{"type":"string","maxLength":100,"description":"Its new name."},"parent_id":{"type":"string","description":"Move it, with everything inside it, into this category, ID from list_categories. It must be the same kind and currency."},"new_parent":{"type":"string","description":"Or into the new category with this key."},"to_top":{"type":"boolean","description":"Move it to the top level."}},"required":["category_id"],"additionalProperties":false}},"edit_rules":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"rule_id":{"type":"string","description":"The Rule to change, ID from list_rules."},"name":{"type":"string","maxLength":100},"match":{"type":"string","enum":["all","any"]},"conditions":{"type":"array","minItems":1,"maxItems":100,"description":"All of its conditions as they should be, same shape as in new_rules: they replace the old ones, so list every condition to keep. Only for a Rule that list_rules shows with \"conditions_complete\": true.","items":{"type":"object","properties":{"field":{"type":"string","enum":["payee","other_party_name","bank_text"]},"test":{"type":"string","enum":["contains","equals","starts_with","ends_with","matches_pattern"],"description":"matches_pattern: text is a pattern in the syntax of Rust's regex crate; lookahead, lookbehind and backreferences are not supported."},"text":{"type":"string","maxLength":1000,"description":"At most 100 characters, but a matches_pattern pattern may have up to 1000."},"case_sensitive":{"type":"boolean","description":"Default false: capitals are ignored, also in a pattern."},"money_in":{"type":"boolean"},"money_out":{"type":"boolean"}},"additionalProperties":false}},"category_id":{"type":"string","description":"The category it books to, ID from list_categories."},"new_category":{"type":"string","description":"Or the key of a category in new_categories."}},"required":["rule_id"],"additionalProperties":false}},"move_rules":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"rule_id":{"type":"string","description":"A top-level Rule, ID from list_rules."},"position":{"type":"integer","minimum":1,"description":"Its new place in the list of top-level Rules, 1 is the first. New Rules in this call are at the end."}},"required":["rule_id","position"],"additionalProperties":false}},"delete_rules":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"rule_id":{"type":"string","description":"The Rule to delete, ID from list_rules. It must have no sub-Rules. What it booked stays as it is."}},"required":["rule_id"],"additionalProperties":false}},"new_transactions":{"type":"array","maxItems":20,"items":{"type":"object","properties":{"account_id":{"type":"string","description":"A cash account of the user, ID from list_accounts."},"date":{"type":"string","description":"The day of the purchase, YYYY-MM-DD."},"amount":{"type":"string","description":"Exact decimal text with a dot, in the account's currency: negative is money out, positive money in, e.g. -12.50."},"payee":{"type":"string","maxLength":200,"description":"Who was paid or who paid, e.g. the shop."},"note":{"type":"string","maxLength":200,"description":"A note, e.g. what the money was for."},"category_id":{"type":"string","description":"The category it belongs to, ID from list_categories, in the account's currency. Leave it out for none yet."}},"required":["account_id","date","amount"],"additionalProperties":false}},"delete_transactions":{"type":"array","maxItems":20,"items":{"type":"object","properties":{"transaction_id":{"type":"string","description":"A transaction of a cash account that the user entered by hand, ID from search_transactions."}},"required":["transaction_id"],"additionalProperties":false}},"edit_transactions":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"transaction_id":{"type":"string","description":"ID from search_transactions."},"date":{"type":"string","description":"The new day, YYYY-MM-DD."},"payee":{"type":"string","maxLength":200,"description":"The new payee: who was paid or who paid. An empty text removes it."},"note":{"type":"string","maxLength":200,"description":"The new note. An empty text removes it."},"amount":{"type":"string","description":"The new amount of the transaction in the account, exact decimal text with a dot, in the transaction's currency: negative is money out, positive money in, e.g. -12.50. Only for a plain transaction with one account and one category."},"account_id":{"type":"string","description":"Move the transaction to this other account, ID from list_accounts, in the same currency. Only for a plain transaction."}},"required":["transaction_id"],"additionalProperties":false}},"categorize":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"transaction_id":{"type":"string","description":"ID from search_transactions."},"category_id":{"type":"string","description":"ID from list_categories."},"new_category":{"type":"string","description":"Or the key of a category in new_categories."}},"required":["transaction_id"],"additionalProperties":false}},"delete_categories":{"type":"array","maxItems":20,"items":{"type":"object","properties":{"category_id":{"type":"string","description":"An empty category to delete, ID from list_categories. It must have no transactions, no sub-categories and no Rule that books to it."}},"required":["category_id"],"additionalProperties":false}},"new_accounts":{"type":"array","maxItems":10,"items":{"type":"object","properties":{"name":{"type":"string","maxLength":100},"type":{"type":"string","enum":["checking","savings","cash","credit_card"]},"currency":{"type":"string","description":"ISO 4217 code. Default: the one all the user's accounts use."}},"required":["name","type"],"additionalProperties":false}},"edit_accounts":{"type":"array","maxItems":10,"items":{"type":"object","properties":{"account_id":{"type":"string","description":"The account to change, ID from list_accounts."},"name":{"type":"string","maxLength":100,"description":"Its new name."},"closed":{"type":"boolean","description":"true closes it (its balance must be zero), false opens a closed one again."},"type":{"type":"string","enum":["checking","savings","cash","credit_card"],"description":"Its new type, e.g. savings for a savings account that came in as checking, or a type for one list_accounts calls other. Only the type changes: transactions and balance stay as they are. To or from credit_card, its balance counts as owed instead of had, or the other way round; net worth stays the same."}},"required":["account_id"],"additionalProperties":false}},"delete_views":{"type":"array","maxItems":20,"items":{"type":"object","properties":{"view_id":{"type":"string","description":"A chart the user saved under Statistics, ID from list_views. Undo brings it back."}},"required":["view_id"],"additionalProperties":false}},"new_budgets":{"type":"array","maxItems":50,"items":{"type":"object","properties":{"name":{"type":"string","maxLength":100,"description":"A name the user asked for, e.g. Holiday, on one line. Leave it out for none: the user then sees its categories."},"category_id":{"type":"string","description":"A spending category without a budget yet, ID from list_categories. The budget counts it with everything inside it."},"category_ids":{"type":"array","maxItems":20,"items":{"type":"string"},"description":"Instead of category_id, for one budget over several spending categories from anywhere in the tree, IDs from list_categories, all in one currency. Each counts with everything inside it."},"account_ids":{"type":"array","maxItems":20,"items":{"type":"string"},"description":"Only count what was paid from these accounts of the user, IDs from list_accounts. Leave it out to count every account."},"every":{"type":"string","enum":["month","quarter","year"],"description":"How often the budget starts again. It starts with the current month, quarter or year."},"salary_day":{"type":"integer","minimum":1,"maximum":31,"description":"Only with every month: the day each month starts, e.g. the payday 25. Default 1."},"based_on":{"type":"string","enum":["last_period","average_3_periods","average_6_periods","average_12_periods"],"description":"Let KassenSturz compute the amount from what the category spent in the last full period, or on average in the last 3, 6 or 12 full periods. Use this whenever the user did not name an amount."},"round_to":{"type":"integer","enum":[1,5,10,50,100],"description":"Only with based_on: round the computed amount up to a multiple of this many whole units of the currency. Default 1."},"amount":{"type":"string","description":"Only for an amount the user named: exact decimal text with a dot, in the category's currency, above zero, e.g. 250.00. Never compute it yourself; use based_on instead."},"carry_over":{"type":"boolean","description":"Whether what is left at the end of a period, or spent too much, moves into the next one. Default false."}},"required":["every"],"additionalProperties":false}},"edit_budgets":{"type":"array","maxItems":50,"items":{"type":"object","properties":{"budget_id":{"type":"string","description":"The budget to change, ID from list_budgets. A change of the amount or carry over applies from the current period on; earlier periods stay as they were."},"name":{"type":"string","maxLength":100,"description":"Its new name, on one line; an empty text takes the name away. Leave it out to keep the name. A new name alone changes no amount."},"based_on":{"type":"string","enum":["last_period","average_3_periods","average_6_periods","average_12_periods"],"description":"Let KassenSturz compute the new amount from what the category spent in the last full period, or on average in the last 3, 6 or 12 full periods."},"round_to":{"type":"integer","enum":[1,5,10,50,100],"description":"Only with based_on: round the computed amount up to a multiple of this many whole units of the currency. Default 1."},"amount":{"type":"string","description":"Only for an amount the user named: exact decimal text with a dot, above zero. Never compute it yourself."},"carry_over":{"type":"boolean","description":"Whether what is left moves into the next period from now on."}},"required":["budget_id"],"additionalProperties":false}}},"additionalProperties":false}"#,
};

/// The most changes one set may hold, so the user can still review it. A
/// set that overhauls fifty Rules, deleting and making each, is 100 changes
/// before anything else (M6-B191, D-627).
pub const MAX_CHANGES: usize = 200;

/// The most transactions one set may add, and the most it may remove (D-258).
const MAX_CASH_CHANGES: usize = 20;

/// The most charts one set may propose (D-282).
const MAX_NEW_VIEWS: usize = 3;

use budgets::MAX_BUDGETS;

/// The lists of [`PROPOSE_CHANGES`] and the permission each needs (D-239).
pub(crate) const SECTIONS: [(&str, Permission); 18] = [
    ("new_categories", Permission::NewCategories),
    ("edit_categories", Permission::EditCategories),
    ("new_rules", Permission::NewRules),
    ("edit_rules", Permission::EditRules),
    ("move_rules", Permission::EditRules),
    ("delete_rules", Permission::DeleteRules),
    ("categorize", Permission::SortTransactions),
    ("delete_categories", Permission::DeleteCategories),
    ("new_accounts", Permission::NewAccounts),
    ("edit_accounts", Permission::EditAccounts),
    ("edit_transactions", Permission::SortTransactions),
    ("new_transactions", Permission::NewTransactions),
    ("delete_transactions", Permission::DeleteTransactions),
    ("new_views", Permission::NewViews),
    ("edit_views", Permission::EditViews),
    ("delete_views", Permission::DeleteViews),
    ("new_budgets", Permission::NewBudgets),
    ("edit_budgets", Permission::EditBudgets),
];

/// [`PROPOSE_CHANGES`] as a model is offered it when the user allows only
/// some kinds of change (D-239): the lists of the others are left out, so
/// the model is never told about what it may not do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposeSpec {
    /// Its name, that of [`PROPOSE_CHANGES`].
    pub name: &'static str,
    /// What it does, for the model.
    pub description: String,
    /// Its arguments as a JSON Schema object.
    pub parameters: String,
}

/// The fields of a transaction `edit_transactions` can change, with the
/// property of the call each one is (D-257). The category is changed by
/// `categorize`.
const EDITABLE_FIELDS: [(Field, &str); 5] = [
    (Field::Amount, "amount"),
    (Field::Date, "date"),
    (Field::Payee, "payee"),
    (Field::Account, "account_id"),
    (Field::Note, "note"),
];

/// Whether the AI may edit transactions under `allowed`: Transactions,
/// Change is on and at least one field isn't locked (D-257).
pub fn edits_allowed(allowed: Permissions) -> bool {
    allowed.allows(Permission::SortTransactions)
        && EDITABLE_FIELDS
            .iter()
            .any(|(field, _)| !allowed.locked(*field))
}

/// The properties of a new transaction the AI may write, with the field
/// each one is (D-258). Without the account, the day and the amount nothing
/// can be added, so a lock on one of them leaves the list out.
const NEW_TRANSACTION_FIELDS: [(Field, &str); 6] = [
    (Field::Account, "account_id"),
    (Field::Date, "date"),
    (Field::Amount, "amount"),
    (Field::Payee, "payee"),
    (Field::Note, "note"),
    (Field::Category, "category_id"),
];

/// Whether the AI may add transactions under `allowed`: Transactions, Add
/// is on and the account, the day and the amount aren't locked (D-258).
pub fn adds_allowed(allowed: Permissions) -> bool {
    allowed.allows(Permission::NewTransactions)
        && ![Field::Account, Field::Date, Field::Amount]
            .into_iter()
            .any(|field| allowed.locked(field))
}

/// Whether the list `name`, which needs `permission`, may be prepared under
/// `allowed`: its cell is on, and for sorting transactions the Category
/// field isn't locked, for editing them some field isn't (D-252, D-257).
fn section_allowed(allowed: Permissions, name: &str, permission: Permission) -> bool {
    match name {
        "categorize" => allowed.sorts(),
        "edit_transactions" => edits_allowed(allowed),
        "new_transactions" => adds_allowed(allowed),
        _ => allowed.allows(permission),
    }
}

/// What a model is offered for preparing changes under `allowed`, or
/// `None` when nothing may be prepared: then the function isn't offered
/// at all, and is refused if it is called (D-239).
pub fn propose_spec(allowed: Permissions) -> Option<ProposeSpec> {
    if !allowed.can_change() {
        return None;
    }
    let mut parameters: Value =
        serde_json::from_str(PROPOSE_CHANGES.parameters).expect("the parameters are JSON");
    let properties = parameters
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .expect("the parameters have properties");
    for (name, permission) in SECTIONS {
        if !section_allowed(allowed, name, permission) {
            properties.remove(name);
        }
    }
    // Charts have a function of their own (D-352): a user who allows only
    // them is offered `propose_view` and not this one.
    if properties.is_empty() {
        return None;
    }
    // A locked field is not offered either (D-257).
    if let Some(fields) = properties
        .get_mut("edit_transactions")
        .and_then(|list| list.get_mut("items"))
        .and_then(|items| items.get_mut("properties"))
        .and_then(Value::as_object_mut)
    {
        for (field, key) in EDITABLE_FIELDS {
            if allowed.locked(field) {
                fields.remove(key);
            }
        }
    }
    if let Some(fields) = properties
        .get_mut("new_transactions")
        .and_then(|list| list.get_mut("items"))
        .and_then(|items| items.get_mut("properties"))
        .and_then(Value::as_object_mut)
    {
        for (field, key) in NEW_TRANSACTION_FIELDS {
            if allowed.locked(field) {
                fields.remove(key);
            }
        }
    }
    if !allowed.allows(Permission::NewCategories) {
        // Nothing to point at: the keys of new categories.
        for list in properties.values_mut() {
            if let Some(item) = list
                .get_mut("items")
                .and_then(|items| items.get_mut("properties"))
                .and_then(Value::as_object_mut)
            {
                item.remove("new_category");
                item.remove("new_parent");
            }
        }
    }
    let kinds: Vec<&str> = [
        (allowed.allows(Permission::NewCategories), "new categories"),
        (
            allowed.allows(Permission::EditCategories),
            "renaming or moving categories",
        ),
        (
            allowed.allows(Permission::NewRules),
            "new Rules that sort future transactions into a category",
        ),
        (
            allowed.allows(Permission::EditRules),
            "changing or moving existing Rules",
        ),
        (allowed.allows(Permission::DeleteRules), "deleting Rules"),
        (
            allowed.sorts(),
            "a category for transactions you found with search_transactions",
        ),
        (
            allowed.allows(Permission::DeleteCategories),
            "deleting categories that nothing uses",
        ),
        (allowed.allows(Permission::NewAccounts), "new accounts"),
        (
            allowed.allows(Permission::EditAccounts),
            "renaming, closing, reopening or changing the type of accounts",
        ),
        (
            edits_allowed(allowed),
            "changing fields of transactions you found with search_transactions",
        ),
        (adds_allowed(allowed), "writing down purchases in cash"),
        (
            allowed.allows(Permission::DeleteTransactions),
            "removing transactions of a cash account that the user entered by hand",
        ),
        (
            allowed.allows(Permission::DeleteViews),
            "deleting saved charts",
        ),
        (allowed.allows(Permission::NewBudgets), "new budgets"),
        (
            allowed.allows(Permission::EditBudgets),
            "changing the name, or the amount or carry over from the current period on, of budgets",
        ),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, text)| text)
    .collect();
    // One line per point, grouped by what it is about, so a model finds the
    // rule for a list without reading one long paragraph (B2 of the US1
    // tool review).
    let mut description = format!(
        "Prepares changes to the user's book for the user to review: {}, and nothing else; \
         the user has not allowed other kinds of change, so don't try them. Nothing changes \
         until the user confirms them in KassenSturz's window, and the user may refuse them.",
        kinds.join(", ")
    );
    let mut general: Vec<&str> = Vec::new();
    let mut transactions: Vec<&str> = Vec::new();
    let mut rules: Vec<&str> = Vec::new();
    let mut budgets: Vec<&str> = Vec::new();
    let mut accounts: Vec<&str> = Vec::new();
    general.push(
        "Categories, Rules and accounts are named by the IDs that list_categories, list_rules \
         and list_accounts give.",
    );
    general.push("Prepare everything that belongs together in one call.");
    if allowed.allows(Permission::NewCategories) {
        general.push(
            "A new category has a key of your choice, which the other lists use as \
             new_category (or new_parent).",
        );
    }
    if allowed.allows(Permission::DeleteRules) || allowed.allows(Permission::DeleteCategories) {
        general
            .push("A Rule or category that a call deletes can't also be changed or moved in it.");
    }
    if allowed.allows(Permission::EditViews) {
        // The IDs of delete_views are explained in its schema (D-681).
        general.push("A chart is prepared or changed (view_id) with propose_view, not here.");
    } else if allowed.allows(Permission::NewViews) {
        general.push("A chart is prepared with propose_view, not here.");
    }
    if allowed.sorts() {
        transactions.push(
            "categorize: only a transaction with one category, or none yet, can get another \
             one; a transfer between the user's own accounts can't.",
        );
    }
    if edits_allowed(allowed) {
        transactions.push(
            "edit_transactions: only the fields it offers can be changed, and the amount and \
             the account only of a plain transaction with one account and one category, never \
             of a transfer.",
        );
    }
    if adds_allowed(allowed) {
        transactions.push(
            "new_transactions: the user's own purchases in cash are written down, only in a \
             cash account (list_accounts gives each account's type); the amount is negative for \
             money paid out.",
        );
    }
    if allowed.allows(Permission::DeleteTransactions) {
        transactions.push(
            "delete_transactions: only a transaction of a cash account that the user entered by \
             hand can be removed, never one a bank delivered.",
        );
    }
    if allowed.allows(Permission::NewRules) {
        rules.push(if allowed.allows(Permission::EditRules) {
            "Rules are checked in order and the first that matches wins, so a new Rule goes last \
             until you move it with move_rules."
        } else {
            "Rules are checked in order and the first that matches wins, so a new Rule goes last."
        });
        rules.push(
            "A condition tests the payee, other_party_name (what the bank named the other \
             party; search_transactions shows it when it differs from the payee) or bank_text, \
             or whether money came in or went out.",
        );
        rules.push(
            "A new Rule can sit inside another (parent_rule_id, or new_parent_rule with the key \
             of a Rule earlier in new_rules): a sub-Rule is checked only when its parent \
             matched.",
        );
        rules.push(
            "One Rule with a matches_pattern condition or many conditions is better than a \
             second Rule with a number in its name.",
        );
    }
    if allowed.allows(Permission::NewRules) && allowed.allows(Permission::DeleteRules) {
        rules.push(
            "A Rule's name is used once among Rules next to each other, ignoring capitals; to \
             replace a Rule by a new one of the same name, delete the old one in the same call.",
        );
    }
    if allowed.allows(Permission::EditRules) {
        rules.push(
            "edit_rules: conditions replace all of a Rule's conditions, so list every one to \
             keep, and only for a Rule that list_rules shows with \"conditions_complete\": true.",
        );
        rules.push("One call can both change and move a Rule.");
    }
    if allowed.allows(Permission::NewBudgets) || allowed.allows(Permission::EditBudgets) {
        budgets.push(
            "A budget is what the user plans to spend on one or more spending categories each \
             month, quarter or year, optionally only what was paid from some of the user's \
             accounts.",
        );
        budgets.push(
            "For its amount, use based_on so that KassenSturz computes it from what the \
             categories spent and shows the user how; give amount only for a figure the user \
             named, and never compute an amount yourself.",
        );
        budgets.push(
            "A budget can have a name, such as Holiday; without one the user sees its \
             categories. Give a name only when the user asked for one.",
        );
        budgets.push("A budget is never deleted here. At most 50 budgets in one call.");
    }
    if allowed.allows(Permission::NewAccounts) {
        accounts
            .push("A new account has only a name, a type and a currency; the user adds the rest.");
    }
    if allowed.allows(Permission::EditAccounts) {
        accounts
            .push("An account is never deleted, only closed, and only when its balance is zero.");
    }
    for (heading, lines) in [
        ("In general", general),
        ("Transactions", transactions),
        ("Rules", rules),
        ("Budgets", budgets),
        ("Accounts", accounts),
    ] {
        if lines.is_empty() {
            continue;
        }
        description.push_str(&format!("\n\n{heading}:"));
        for line in lines {
            description.push_str("\n- ");
            description.push_str(line);
        }
    }
    Some(ProposeSpec {
        name: PROPOSE_CHANGES.name,
        description,
        parameters: parameters.to_string(),
    })
}

/// The most characters of a name or a condition's text.
const MAX_NAME_CHARS: usize = 100;

/// The most characters of a new category's key.
const MAX_KEY_CHARS: usize = 40;

/// The most conditions of one Rule. The app sets no limit; this one keeps
/// the review readable, and a pattern condition replaces many others
/// (M6-B191, D-627).
const MAX_CONDITIONS: usize = 100;

/// The most characters of a pattern, against 100 for any other text.
const MAX_PATTERN_CHARS: usize = 1000;

/// The names of every argument [`PROPOSE_CHANGES`] takes, at any depth:
/// the keywords of `ai-tools.bad-argument` for it.
#[cfg(test)]
pub(crate) const ARGUMENTS: &[&str] = &[
    "edit_categories",
    "new_parent",
    "to_top",
    "edit_rules",
    "rule_id",
    "move_rules",
    "position",
    "delete_rules",
    "new_categories",
    "key",
    "name",
    "parent_rule_id",
    "new_parent_rule",
    "parent_id",
    "income",
    "currency",
    "new_rules",
    "match",
    "conditions",
    "field",
    "test",
    "text",
    "case_sensitive",
    "money_in",
    "money_out",
    "category_id",
    "new_category",
    "categorize",
    "transaction_id",
    "delete_categories",
    "new_accounts",
    "type",
    "edit_accounts",
    "account_id",
    "closed",
    "edit_transactions",
    "new_transactions",
    "delete_transactions",
    "date",
    "payee",
    "note",
    "amount",
    "account_id",
    "new_views",
    "edit_views",
    "delete_views",
    "view_id",
    "title",
    "dataset",
    "chart",
    "grouping",
    "measure",
    "compare",
    "period",
    "new_budgets",
    "edit_budgets",
    "budget_id",
    "category_ids",
    "account_ids",
    "every",
    "salary_day",
    "based_on",
    "round_to",
    "carry_over",
];

/// One change of a set, as the user reviews it. Every text in it came from
/// the model or the book: shown as plain text (threat T-08).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposedChange {
    /// A new category.
    NewCategory(NewCategory),
    /// A category renamed, moved or both.
    EditCategory(EditCategory),
    /// A new Rule, at the end of the list.
    NewRule(NewRule),
    /// An existing Rule changed.
    EditRule(EditRule),
    /// A top-level Rule moved to another place in the list.
    MoveRule(MoveRule),
    /// A Rule deleted.
    DeleteRule(DeleteRule),
    /// A transaction booked to another category, or to one for the first
    /// time.
    Categorize(Categorize),
    /// An empty category deleted.
    DeleteCategory(DeleteCategory),
    /// A new account.
    NewAccount(NewAccount),
    /// An account renamed, closed or opened again.
    EditAccount(EditAccount),
    /// A transaction with some fields changed (D-257).
    EditTransaction(EditTransaction),
    /// A payment in cash written down (D-258).
    NewTransaction(NewTransaction),
    /// A transaction of a cash account removed (D-258).
    DeleteTransaction(DeleteTransaction),
    /// A chart the user may save (M6a.9, D-282).
    NewView(NewView),
    /// A saved chart changed (D-681).
    EditView(EditView),
    /// A saved chart deleted (D-681).
    DeleteView(DeleteView),
    /// A new budget (M6c.11, D-358).
    NewBudget(NewBudget),
    /// A budget's amount or carry over changed from one period on
    /// (M6c.11, D-358).
    EditBudget(EditBudget),
}

/// A new category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCategory {
    /// The ID it will have.
    pub id: AccountId,
    /// Its name, with the names of the categories above it first.
    pub path: Vec<String>,
    /// The category it goes inside, if any.
    pub parent: Option<AccountId>,
    /// An income category, or a spending one.
    pub income: bool,
    /// Its currency.
    pub currency: CurrencyCode,
}

/// A category renamed, moved or both. Its subcategories and transactions go
/// with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditCategory {
    /// The category.
    pub id: AccountId,
    /// Its path before: the names above it first, its own last.
    pub before: Vec<String>,
    /// Its path after.
    pub after: Vec<String>,
    /// Its new name, if it is renamed.
    name: Option<String>,
    /// Its new place, if it moves: the category it goes inside, `None` for
    /// the top level.
    parent: Option<Option<AccountId>>,
    /// The category as the user reviewed it: applying fails if it changed
    /// since.
    reviewed: Account,
}

/// An existing Rule changed. What it booked earlier stays as it is (Q-044).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditRule {
    /// The Rule as it was.
    pub before: Rule,
    /// The category it booked to before, as a path, if it booked to one.
    pub before_category: Option<Vec<String>>,
    /// The Rule as it will be.
    pub rule: Rule,
    /// The category it books to, as a path, if it books to one.
    pub category: Option<Vec<String>>,
}

/// A top-level Rule moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveRule {
    /// The Rule.
    pub id: RuleId,
    /// Its name.
    pub name: String,
    /// Its place before, counted from 1 among the top-level Rules.
    pub from: usize,
    /// Its place after.
    pub to: usize,
}

/// A Rule deleted. What it booked stays as it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteRule {
    /// The Rule as it is, so the user sees what goes.
    pub rule: Rule,
    /// The category it books to, as a path, if it books to one.
    pub category: Option<Vec<String>>,
}

/// A new Rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRule {
    /// The Rule, which books to a category.
    pub rule: Rule,
    /// That category's path, new or not.
    pub category: Vec<String>,
    /// The Rule it sits inside, if it is a sub-Rule: one of the book's, or a
    /// new one earlier in the set (M6-B191, D-628).
    pub parent: Option<RuleId>,
    /// That Rule's name, for the review.
    pub inside: Option<String>,
}

/// A transaction that gets a category.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Categorize {
    /// The transaction.
    pub transaction: TransactionId,
    /// The day the user's account shows it.
    pub date: Date,
    /// Its payee, if any.
    pub payee: Option<String>,
    /// The user's account the money moved in or out of.
    pub account: String,
    /// What it did to that account, in the account's currency: positive
    /// for money in (D-065: the front end writes it as text).
    pub amount: Money,
    /// The category's path before, `None` for "no category yet".
    pub before: Option<Vec<String>>,
    /// The category's path after.
    pub after: Vec<String>,
    /// The category it gets.
    pub category: AccountId,
    /// The transaction as the user reviewed it: applying fails if it
    /// changed since.
    reviewed: Transaction,
    /// Its part that gets the category.
    part: SplitId,
}

/// The changes a model prepared, checked against the book. Only
/// [`propose`] makes one, and only [`ChangeSet::apply`] changes the book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSet {
    changes: Vec<ProposedChange>,
    /// The top-level Rules in the order the set was checked against: moving
    /// and deleting Rules applies only to a list in this order.
    rule_order: Vec<RuleId>,
    /// Where each change stands in the model's call, in the order of
    /// `changes` (D-297).
    slots: Vec<Slot>,
    /// The model's call and what the user changed in it (D-293).
    call: edits::Call,
}

/// The user's yes to one change set. Only the confirm button of the
/// KassenSturz window makes one (D-171): never the chat, the MCP server or
/// anything a model can reach, which M5.12 checks.
#[derive(Debug)]
pub struct Confirmed(());

impl Confirmed {
    /// The user pressed the confirm button for the set on the screen.
    pub fn by_the_user() -> Self {
        Self(())
    }
}

/// What the call prepared: the set for the user, and the answer for the
/// model, which says that it waits for the user.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    /// The set to review.
    pub changes: ChangeSet,
    /// The model's answer to its call.
    pub output: Output,
}

/// Why a change set wasn't applied. Nothing of it was: the book is as it
/// was before.
#[derive(Debug)]
pub enum ChangeError {
    /// The book changed since the set was prepared: a transaction in it
    /// was changed or removed.
    OutOfDate,
    /// The book refused a change, e.g. because the user added a category
    /// of the same name since.
    NotApplied(StoreError),
    /// The set holds a kind of change the user doesn't allow now: they
    /// turned it off after it was prepared (D-239).
    NotAllowed,
}

impl From<StoreError> for ChangeError {
    fn from(error: StoreError) -> Self {
        Self::NotApplied(error)
    }
}

impl core::fmt::Display for ChangeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.report().code)
    }
}

impl std::error::Error for ChangeError {}

impl Report for ChangeError {
    fn report(&self) -> ErrorReport {
        match self {
            Self::OutOfDate => ErrorReport::new("ai-changes.out-of-date"),
            Self::NotAllowed => ErrorReport::new("ai-changes.not-allowed"),
            Self::NotApplied(error) => {
                ErrorReport::new("ai-changes.not-applied").caused_by(error.report())
            }
        }
    }
}

impl ChangeSet {
    /// The changes, in the order they are applied: new categories, edited
    /// categories, new Rules, edited, deleted and moved Rules, then the
    /// transactions.
    pub fn changes(&self) -> &[ProposedChange] {
        &self.changes
    }

    /// Whether every change in the set is of a kind the user allows under
    /// `allowed`. The chat and the request files check this again when the
    /// user confirms, so turning a permission off also stops what was
    /// prepared earlier (D-239).
    pub fn check_allowed(&self, allowed: Permissions) -> Result<(), ChangeError> {
        let permitted = self.changes.iter().all(|change| match change {
            ProposedChange::NewCategory(_) => allowed.allows(Permission::NewCategories),
            ProposedChange::EditCategory(_) => allowed.allows(Permission::EditCategories),
            ProposedChange::NewRule(_) => allowed.allows(Permission::NewRules),
            ProposedChange::EditRule(_) | ProposedChange::MoveRule(_) => {
                allowed.allows(Permission::EditRules)
            }
            ProposedChange::DeleteRule(_) => allowed.allows(Permission::DeleteRules),
            ProposedChange::Categorize(_) => allowed.sorts(),
            ProposedChange::EditTransaction(edit) => {
                allowed.allows(Permission::SortTransactions)
                    && edit.fields().all(|field| !allowed.locked(field))
            }
            ProposedChange::DeleteCategory(_) => allowed.allows(Permission::DeleteCategories),
            ProposedChange::NewAccount(_) => allowed.allows(Permission::NewAccounts),
            ProposedChange::EditAccount(_) => allowed.allows(Permission::EditAccounts),
            ProposedChange::NewTransaction(_) => adds_allowed(allowed),
            ProposedChange::DeleteTransaction(_) => allowed.allows(Permission::DeleteTransactions),
            ProposedChange::NewView(_) => allowed.allows(Permission::NewViews),
            ProposedChange::EditView(_) => allowed.allows(Permission::EditViews),
            ProposedChange::DeleteView(_) => allowed.allows(Permission::DeleteViews),
            ProposedChange::NewBudget(_) => allowed.allows(Permission::NewBudgets),
            ProposedChange::EditBudget(_) => allowed.allows(Permission::EditBudgets),
        });
        if permitted {
            Ok(())
        } else {
            Err(ChangeError::NotAllowed)
        }
    }

    /// The set in a line of plain words for the developer mode's log: what
    /// each change does, with the names and paths the user reads on the
    /// review page.
    pub fn summary(&self) -> String {
        let path = |path: &[String]| path.join(" › ");
        let book_to = |category: &Option<Vec<String>>| {
            category
                .as_deref()
                .map_or_else(String::new, |category| format!(" to {}", path(category)))
        };
        let parts: Vec<String> = self
            .changes
            .iter()
            .map(|change| match change {
                ProposedChange::NewCategory(new) => {
                    let kind = if new.income { "income" } else { "spending" };
                    format!("new {kind} category \"{}\"", path(&new.path))
                }
                ProposedChange::EditCategory(edit) => format!(
                    "category \"{}\" becomes \"{}\"",
                    path(&edit.before),
                    path(&edit.after)
                ),
                ProposedChange::NewRule(new) => format!(
                    "new Rule \"{}\"{} booking to {}",
                    new.rule.name,
                    new.inside
                        .as_ref()
                        .map_or_else(String::new, |inside| format!(" inside \"{inside}\"")),
                    path(&new.category)
                ),
                ProposedChange::EditRule(edit) => format!(
                    "Rule \"{}\" becomes \"{}\"{}",
                    edit.before.name,
                    edit.rule.name,
                    book_to(&edit.category)
                ),
                ProposedChange::MoveRule(moved) => format!(
                    "Rule \"{}\" moves from place {} to {}",
                    moved.name, moved.from, moved.to
                ),
                ProposedChange::DeleteRule(delete) => {
                    format!("delete Rule \"{}\"", delete.rule.name)
                }
                ProposedChange::Categorize(change) => format!(
                    "transaction {} gets category \"{}\"",
                    change.transaction,
                    path(&change.after)
                ),
                ProposedChange::DeleteCategory(delete) => {
                    format!("delete category \"{}\"", path(&delete.path))
                }
                ProposedChange::NewAccount(new) => format!("new account \"{}\"", new.name),
                ProposedChange::EditAccount(edit) => {
                    let mut parts = Vec::new();
                    if edit.renames() {
                        parts.push(format!("becomes \"{}\"", edit.after));
                    }
                    if edit.closed != edit.was_closed {
                        parts.push(
                            if edit.closed {
                                "is closed"
                            } else {
                                "is opened again"
                            }
                            .to_owned(),
                        );
                    }
                    if edit.changes_type() {
                        let name = |t: Option<kassensturz_core::AccountType>| {
                            t.map_or("other", crate::read::account_type)
                        };
                        parts.push(format!(
                            "changes type from {} to {}{}",
                            name(edit.type_before),
                            name(edit.account_type),
                            if edit.changes_kind() {
                                " (its balance now counts as owed instead of had, or the \
                                 other way round; net worth stays the same)"
                            } else {
                                ""
                            }
                        ));
                    }
                    format!("account \"{}\" {}", edit.before, parts.join(" and "))
                }
                ProposedChange::EditTransaction(edit) => format!(
                    "transaction {} changes {}",
                    edit.transaction,
                    edit.fields()
                        .map(Field::name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                ProposedChange::NewTransaction(new) => format!(
                    "new transaction {} in cash account \"{}\"",
                    new.transaction, new.account_name
                ),
                ProposedChange::DeleteTransaction(delete) => {
                    format!("transaction {} is removed", delete.transaction)
                }
                ProposedChange::NewView(new) => format!(
                    "new chart \"{}\" ({} of {}, {})",
                    new.title,
                    new.type_id(),
                    new.dataset_id(),
                    new.period
                ),
                ProposedChange::EditView(edit) => format!(
                    "chart \"{}\" changes to \"{}\" ({} of {}, {})",
                    edit.before,
                    edit.title,
                    edit.type_id(),
                    edit.dataset_id(),
                    edit.period
                ),
                ProposedChange::DeleteView(delete) => {
                    format!("chart \"{}\" is deleted", delete.name)
                }
                ProposedChange::NewBudget(new) => format!(
                    "new budget{} for {} every {} from {}{}",
                    budgets::name_text(new.name.as_deref()),
                    budgets::paths_text(&new.paths),
                    every_word(new.every),
                    new.first_day,
                    budgets::accounts_text(&new.account_names)
                ),
                ProposedChange::EditBudget(edit) => format!(
                    "budget{} for {} changes{}{}",
                    budgets::name_text(edit.name_before.as_deref()),
                    budgets::paths_text(&edit.paths),
                    match &edit.rename {
                        Some(Some(name)) => format!(" its name to \"{name}\""),
                        Some(None) => " to have no name".to_owned(),
                        None => String::new(),
                    },
                    if edit.changes_amount {
                        format!(" from {}", edit.from_day)
                    } else {
                        String::new()
                    }
                ),
            })
            .collect();
        parts.join("; ")
    }

    /// Applies the set to `book` as one user action, which one undo takes
    /// back (D-198). Everything is applied or nothing: if the book changed
    /// since the set was prepared, or the book refuses one of the changes,
    /// the book stays as it was.
    pub fn apply(self, book: &mut Book, confirmed: Confirmed) -> Result<(), ChangeError> {
        let Confirmed(()) = confirmed;
        // The user left every change out (D-293): nothing to make, and no
        // empty entry in the history.
        if self.changes.is_empty() {
            return Ok(());
        }
        let moves = self
            .changes
            .iter()
            .any(|change| matches!(change, ProposedChange::MoveRule(_)));
        if moves && top_level_rules(book)? != self.rule_order {
            return Err(ChangeError::OutOfDate);
        }
        book.as_one_action(Action::ApplyAiChanges, |book| {
            for change in &self.changes {
                match change {
                    ProposedChange::NewCategory(new) => {
                        book.use_currency(new.currency)?;
                        book.add_account(&Account {
                            id: new.id,
                            name: new.path.last().cloned().unwrap_or_default(),
                            kind: if new.income {
                                AccountKind::Income
                            } else {
                                AccountKind::Expense
                            },
                            currency: new.currency,
                            account_type: None,
                            parent: new.parent,
                            closed: false,
                            number: None,
                            notes: None,
                            institution: None,
                        })?;
                    }
                    ProposedChange::EditCategory(edit) => {
                        if book.account(edit.id)? != edit.reviewed {
                            return Err(ChangeError::OutOfDate);
                        }
                        if let Some(parent) = edit.parent {
                            book.move_category(edit.id, parent)?;
                        }
                        if let Some(name) = &edit.name {
                            book.rename_account(edit.id, name)?;
                        }
                    }
                    ProposedChange::NewRule(new) => book.add_rule(&new.rule, new.parent)?,
                    ProposedChange::EditRule(edit) => {
                        if rule_in_book(book, edit.before.id)? != Some(edit.before.clone()) {
                            return Err(ChangeError::OutOfDate);
                        }
                        book.update_rule(&edit.rule)?;
                    }
                    ProposedChange::DeleteRule(delete) => {
                        if rule_in_book(book, delete.rule.id)? != Some(delete.rule.clone()) {
                            return Err(ChangeError::OutOfDate);
                        }
                        book.remove_rule(delete.rule.id)?;
                    }
                    ProposedChange::MoveRule(moved) => {
                        let order = top_level_rules(book)?;
                        if order.get(moved.from - 1) != Some(&moved.id) {
                            return Err(ChangeError::OutOfDate);
                        }
                        book.move_rule(moved.id, None, moved.to - 1)?;
                    }
                    ProposedChange::Categorize(change) => {
                        let stored = match book.transaction(change.transaction) {
                            Err(StoreError::UnknownTransaction(_)) => {
                                return Err(ChangeError::OutOfDate);
                            }
                            other => other?,
                        };
                        if stored != change.reviewed {
                            return Err(ChangeError::OutOfDate);
                        }
                        book.update_transaction(&recategorized(change)?)?;
                    }
                    ProposedChange::NewAccount(new) => accounts::apply_new_account(book, new)?,
                    ProposedChange::EditAccount(edit) => accounts::apply_edit_account(book, edit)?,
                    ProposedChange::DeleteCategory(delete) => {
                        accounts::apply_delete_category(book, delete)?;
                    }
                    ProposedChange::EditTransaction(edit) => {
                        let stored = match book.transaction(edit.transaction) {
                            Err(StoreError::UnknownTransaction(_)) => {
                                return Err(ChangeError::OutOfDate);
                            }
                            other => other?,
                        };
                        if stored != edit.reviewed {
                            return Err(ChangeError::OutOfDate);
                        }
                        book.update_transaction(&edit.edited)?;
                    }
                    ProposedChange::NewTransaction(new) => book.add_transaction(&new.built)?,
                    ProposedChange::DeleteTransaction(delete) => {
                        let stored = match book.transaction(delete.transaction) {
                            Err(StoreError::UnknownTransaction(_)) => {
                                return Err(ChangeError::OutOfDate);
                            }
                            other => other?,
                        };
                        if stored != delete.reviewed {
                            return Err(ChangeError::OutOfDate);
                        }
                        book.remove_transaction(delete.transaction)?;
                    }
                    ProposedChange::NewView(new) => {
                        book.add_view(&new.document)?;
                    }
                    ProposedChange::EditView(edit) => views::apply_edit_view(book, edit)?,
                    ProposedChange::DeleteView(delete) => views::apply_delete_view(book, delete)?,
                    ProposedChange::NewBudget(new) => budgets::apply_new_budget(book, new)?,
                    ProposedChange::EditBudget(edit) => budgets::apply_edit_budget(book, edit)?,
                }
            }
            Ok(())
        })
    }
}

/// The IDs of the top-level Rules, in the user's order.
fn top_level_rules(book: &Book) -> Result<Vec<RuleId>, StoreError> {
    Ok(book.rules()?.into_iter().map(|node| node.rule.id).collect())
}

/// The Rule with this ID, if the book has it.
fn rule_in_book(book: &Book, id: RuleId) -> Result<Option<Rule>, StoreError> {
    match book.rule(id) {
        Err(StoreError::UnknownRule(_)) => Ok(None),
        other => other.map(Some),
    }
}

/// The reviewed transaction with the part booked to the new category.
fn recategorized(change: &Categorize) -> Result<Transaction, ChangeError> {
    let before = &change.reviewed;
    let splits = before
        .splits()
        .iter()
        .map(|split| {
            if split.id == change.part {
                Split {
                    account: Some(change.category),
                    // A category in the transaction's currency (checked when
                    // proposed) takes no amount in another one.
                    foreign: None,
                    ..split.clone()
                }
            } else {
                split.clone()
            }
        })
        .collect();
    Transaction::new(
        before.id(),
        before.booking_date(),
        before.payee().map(str::to_owned),
        splits,
    )
    .map(|transaction| transaction.with_details_of(before))
    .map_err(|_| ChangeError::OutOfDate)
}

/// Checks a model's call of [`PROPOSE_CHANGES`] against `book` and turns it
/// into a change set for the user to review. The book doesn't change. A
/// list of a kind the user doesn't allow under `allowed` is refused, not
/// left out (D-239). Charts are not among its lists since `propose_view`
/// replaced `new_views` (D-352), so a call with that list is refused.
pub fn propose(
    book: &Book,
    arguments: &Value,
    allowed: Permissions,
) -> Result<Proposal, ToolError> {
    refuse_views(arguments)?;
    propose_with(book, arguments, allowed, None)
}

/// A model's call of [`PROPOSE_CHANGES`] may not name `new_views` or
/// `edit_views`: charts are prepared and changed with their own function
/// since M6c.8 (D-352, D-681).
fn refuse_views(arguments: &Value) -> Result<(), ToolError> {
    if arguments.get("new_views").is_some() || arguments.get("edit_views").is_some() {
        return Err(ToolError::UnknownArgument);
    }
    Ok(())
}

/// [`propose`] on `today`, the day of this computer's clock: budgets
/// start with the period that contains it and are changed from it on, so
/// only a call that knows the day may prepare them (D-394).
pub fn propose_on(
    book: &Book,
    arguments: &Value,
    allowed: Permissions,
    today: Date,
) -> Result<Proposal, ToolError> {
    refuse_views(arguments)?;
    propose_with(book, arguments, allowed, Some(today))
}

/// Checks a call of one of the two functions that prepare changes, by its
/// name: [`PROPOSE_CHANGES`] (on `today`) or `propose_view`. The MCP server
/// and the request files use it, since a request names its function
/// (D-352, D-393).
pub fn prepare_named(
    book: &Book,
    name: &str,
    arguments: &Value,
    allowed: Permissions,
    today: Date,
) -> Result<Proposal, ToolError> {
    match name {
        _ if name == PROPOSE_CHANGES.name => propose_on(book, arguments, allowed, today),
        PROPOSE_VIEW_NAME => propose_view(book, arguments, allowed, today),
        _ => Err(ToolError::UnknownFunction),
    }
}

/// Prepares a change set from a call of the lists of [`build`] without a
/// day: the one `propose_view` makes of its call, with the chart in
/// `new_views`, and the tests of the lists.
fn prepare(book: &Book, arguments: &Value, allowed: Permissions) -> Result<Proposal, ToolError> {
    propose_with(book, arguments, allowed, None)
}

fn propose_with(
    book: &Book,
    arguments: &Value,
    allowed: Permissions,
    today: Option<Date>,
) -> Result<Proposal, ToolError> {
    let Built {
        changes,
        rule_order,
    } = build(book, arguments, allowed, today)?;
    if changes.is_empty() {
        return Err(ToolError::BadArgument {
            argument: "arguments",
        });
    }
    let mut figures = Figures::new();
    let total = u64::try_from(changes.len()).unwrap_or(u64::MAX);
    figures.add_count(total);
    let output = Output::new(
        json!({
            "prepared": total,
            "status": "waiting for the user's review; nothing is changed until the user confirms",
        }),
        figures,
    );
    let (slots, call) = edits::Call::first(arguments, &changes, today);
    Ok(Proposal {
        changes: ChangeSet {
            changes,
            rule_order,
            slots,
            call,
        },
        output,
    })
}

/// The changes a checked call makes, and the order of the top-level Rules
/// they were checked against.
struct Built {
    changes: Vec<ProposedChange>,
    rule_order: Vec<RuleId>,
}

/// Checks a call of [`PROPOSE_CHANGES`] against `book` and turns it into
/// changes, one for each item of its lists, in the order of
/// [`ChangeSet::changes`]: the model's call, and the same call as the user
/// edited it (D-293), go through these same checks. A call with no item is
/// not refused here, since the user may leave every change out.
fn build(
    book: &Book,
    arguments: &Value,
    allowed: Permissions,
    today: Option<Date>,
) -> Result<Built, ToolError> {
    let call = Object::read(
        arguments,
        &[
            "new_categories",
            "edit_categories",
            "new_rules",
            "edit_rules",
            "move_rules",
            "delete_rules",
            "categorize",
            "delete_categories",
            "new_accounts",
            "edit_accounts",
            "edit_transactions",
            "new_transactions",
            "delete_transactions",
            "new_views",
            "edit_views",
            "delete_views",
            "new_budgets",
            "edit_budgets",
        ],
        "arguments",
    )?;
    let categories = call.array("new_categories")?;
    let edited_categories = call.array("edit_categories")?;
    let rules = call.array("new_rules")?;
    let edited_rules = call.array("edit_rules")?;
    let moved_rules = call.array("move_rules")?;
    let deleted_rules = call.array("delete_rules")?;
    let transactions = call.array("categorize")?;
    let deleted_categories = call.array("delete_categories")?;
    let new_accounts = call.array("new_accounts")?;
    let edited_accounts = call.array("edit_accounts")?;
    let edited_transactions = call.array("edit_transactions")?;
    let new_transactions = call.array("new_transactions")?;
    let deleted_transactions = call.array("delete_transactions")?;
    let new_views = call.array("new_views")?;
    let edited_views = call.array("edit_views")?;
    let deleted_views = call.array("delete_views")?;
    let new_budgets = call.array("new_budgets")?;
    let edited_budgets = call.array("edit_budgets")?;
    for (name, permission) in SECTIONS {
        if !section_allowed(allowed, name, permission) && !call.array(name)?.is_empty() {
            return Err(ToolError::NotAllowed { argument: name });
        }
    }
    let count = categories.len()
        + edited_categories.len()
        + rules.len()
        + edited_rules.len()
        + moved_rules.len()
        + deleted_rules.len()
        + transactions.len()
        + deleted_categories.len()
        + new_accounts.len()
        + edited_accounts.len()
        + edited_transactions.len()
        + new_transactions.len()
        + deleted_transactions.len()
        + new_views.len()
        + edited_views.len()
        + deleted_views.len()
        + new_budgets.len()
        + edited_budgets.len();
    if count > MAX_CHANGES || new_budgets.len() + edited_budgets.len() > MAX_BUDGETS {
        return Err(ToolError::TooManyChanges);
    }

    let mut known = Known::read(book)?;
    let rule_order = known.rule_order.clone();
    let mut changes = Vec::with_capacity(count);
    for (index, category) in categories.iter().enumerate() {
        changes.push(ProposedChange::NewCategory(
            known
                .new_category(category)
                .map_err(|e| e.at("new_categories", index))?,
        ));
    }
    let mut edited = HashSet::new();
    for (index, item) in edited_categories.iter().enumerate() {
        let change = known
            .edit_category(item)
            .map_err(|e| e.at("edit_categories", index))?;
        if !edited.insert(change.id) {
            return Err(ToolError::BadArgument {
                argument: "category_id",
            }
            .at("edit_categories", index));
        }
        changes.push(ProposedChange::EditCategory(change));
    }
    // Changing and deleting Rules come before new ones, in the check and
    // when the set is applied: a Rule can be replaced by a new one of the
    // same name in one call (M6-B191, D-626).
    let mut touched = RuleTouches::default();
    for (index, item) in edited_rules.iter().enumerate() {
        changes.push(ProposedChange::EditRule(
            known
                .edit_rule(item, &mut touched)
                .map_err(|e| e.at("edit_rules", index))?,
        ));
    }
    for (index, item) in deleted_rules.iter().enumerate() {
        changes.push(ProposedChange::DeleteRule(
            known
                .delete_rule(item, &mut touched)
                .map_err(|e| e.at("delete_rules", index))?,
        ));
    }
    for (index, rule) in rules.iter().enumerate() {
        let new = known.new_rule(rule).map_err(|e| e.at("new_rules", index))?;
        if new.parent.is_none() {
            known.rule_order.push(new.rule.id);
        }
        changes.push(ProposedChange::NewRule(new));
    }
    for (index, item) in moved_rules.iter().enumerate() {
        changes.push(ProposedChange::MoveRule(
            known
                .move_rule(item, &mut touched)
                .map_err(|e| e.at("move_rules", index))?,
        ));
    }
    let mut seen = HashSet::new();
    for (index, item) in transactions.iter().enumerate() {
        let change = known
            .categorize(book, item)
            .map_err(|e| e.at("categorize", index))?;
        if !seen.insert(change.transaction) {
            return Err(ToolError::BadArgument {
                argument: "transaction_id",
            }
            .at("categorize", index));
        }
        changes.push(ProposedChange::Categorize(change));
    }
    // After the rest, so what the set moves or books away is seen as it
    // will be.
    for (index, item) in new_accounts.iter().enumerate() {
        changes.push(ProposedChange::NewAccount(
            known
                .new_account(item)
                .map_err(|e| e.at("new_accounts", index))?,
        ));
    }
    let mut edited_account_ids = HashSet::new();
    for (index, item) in edited_accounts.iter().enumerate() {
        changes.push(ProposedChange::EditAccount(
            known
                .edit_account(book, item, &mut edited_account_ids)
                .map_err(|e| e.at("edit_accounts", index))?,
        ));
    }
    let mut gone = HashSet::new();
    for (index, item) in deleted_categories.iter().enumerate() {
        let change = known
            .delete_category(book, item, &mut gone)
            .map_err(|e| e.at("delete_categories", index))?;
        // A category this set renames or moves isn't deleted by it too,
        // like a Rule (M6-B196): the review would show a change that never
        // lasts.
        if edited.contains(&change.id) {
            return Err(ToolError::NotChangeable {
                argument: "category_id",
                reason: "also-deleted",
            }
            .at("delete_categories", index));
        }
        changes.push(ProposedChange::DeleteCategory(change));
    }
    for (index, item) in edited_transactions.iter().enumerate() {
        let change = known
            .edit_transaction(book, item, allowed)
            .map_err(|e| e.at("edit_transactions", index))?;
        if !seen.insert(change.transaction) {
            return Err(ToolError::BadArgument {
                argument: "transaction_id",
            }
            .at("edit_transactions", index));
        }
        changes.push(ProposedChange::EditTransaction(change));
    }
    if new_transactions.len() > MAX_CASH_CHANGES || deleted_transactions.len() > MAX_CASH_CHANGES {
        return Err(ToolError::TooManyChanges);
    }
    for (index, item) in new_transactions.iter().enumerate() {
        changes.push(ProposedChange::NewTransaction(
            known
                .new_transaction(book, item, allowed)
                .map_err(|e| e.at("new_transactions", index))?,
        ));
    }
    for (index, item) in deleted_transactions.iter().enumerate() {
        let change = known
            .delete_transaction(book, item)
            .map_err(|e| e.at("delete_transactions", index))?;
        if !seen.insert(change.transaction) {
            return Err(ToolError::BadArgument {
                argument: "transaction_id",
            }
            .at("delete_transactions", index));
        }
        changes.push(ProposedChange::DeleteTransaction(change));
    }
    if new_views.len() + edited_views.len() > MAX_NEW_VIEWS {
        return Err(ToolError::TooManyChanges);
    }
    let mut view_names = HashSet::new();
    for (index, item) in new_views.iter().enumerate() {
        changes.push(ProposedChange::NewView(
            views::new_view(book, item, &mut view_names).map_err(|e| e.at("new_views", index))?,
        ));
    }
    let mut touched_views = HashSet::new();
    for (index, item) in edited_views.iter().enumerate() {
        changes.push(ProposedChange::EditView(
            views::edit_view(book, item, &mut view_names, &mut touched_views)
                .map_err(|e| e.at("edit_views", index))?,
        ));
    }
    for (index, item) in deleted_views.iter().enumerate() {
        changes.push(ProposedChange::DeleteView(
            views::delete_view(book, item, &mut touched_views)
                .map_err(|e| e.at("delete_views", index))?,
        ));
    }

    if !(new_budgets.is_empty() && edited_budgets.is_empty()) {
        // Budgets start with, and change from, the current period: only a
        // caller that knows the day may prepare them (D-394).
        let today = today.ok_or(ToolError::BadArgument {
            argument: if new_budgets.is_empty() {
                "edit_budgets"
            } else {
                "new_budgets"
            },
        })?;
        let stored = book.budgets()?;
        let mut prepared = Vec::new();
        for (index, item) in new_budgets.iter().enumerate() {
            changes.push(ProposedChange::NewBudget(
                budgets::new_budget(book, item, today, &stored, &mut prepared)
                    .map_err(|e| e.at("new_budgets", index))?,
            ));
        }
        let mut edited_ids = Vec::new();
        for (index, item) in edited_budgets.iter().enumerate() {
            changes.push(ProposedChange::EditBudget(
                budgets::edit_budget(book, item, today, &stored, &mut edited_ids)
                    .map_err(|e| e.at("edit_budgets", index))?,
            ));
        }
    }

    // The paths the user reads are those after all the moves and renames.
    known.refresh_paths();
    for change in &mut changes {
        match change {
            ProposedChange::NewCategory(new) => new.path = known.path(new.id),
            ProposedChange::EditCategory(edit) => edit.after = known.path(edit.id),
            ProposedChange::NewRule(new) => {
                new.category = known.rule_category(&new.rule).unwrap_or_default();
            }
            ProposedChange::EditRule(edit) => edit.category = known.rule_category(&edit.rule),
            _ => {}
        }
    }
    Ok(Built {
        changes,
        rule_order,
    })
}

/// What the checks need to know about the book, and the categories the set
/// adds so far.
struct Known {
    accounts: HashMap<AccountId, Account>,
    paths: HashMap<AccountId, Vec<String>>,
    /// The paths as the book has them, before any change of the set.
    original_paths: HashMap<AccountId, Vec<String>>,
    /// The new categories by their keys.
    keys: HashMap<String, AccountId>,
    /// The currency of the user's open accounts, if they all use one.
    usual_currency: Option<CurrencyCode>,
    /// Every Rule of the book.
    rules: HashMap<RuleId, RuleInfo>,
    /// The top-level Rules in order, as the set so far leaves them.
    rule_order: Vec<RuleId>,
    /// The names the Rules have, ignoring capitals, in each place (the top
    /// level or inside a Rule), as the set so far leaves them: a Rule the
    /// set deletes or renames frees its name for the next one (M6-B191).
    rule_names: HashMap<Option<RuleId>, HashSet<String>>,
    /// The new Rules by their keys: the Rule, its name and how deep it sits
    /// (M6-B191, D-628).
    rule_keys: HashMap<String, (RuleId, String, usize)>,
    /// The Rules the set deletes so far: nothing can go inside them.
    deleted_rules: HashSet<RuleId>,
}

/// A Rule of the book and where it sits.
struct RuleInfo {
    rule: Rule,
    parent: Option<RuleId>,
    sub_rules: usize,
}

/// What a change set does to an existing Rule.
#[derive(Clone, Copy)]
enum RuleTouch {
    Edit,
    Move,
    Delete,
}

/// The Rules a change set already changes, moves or deletes (M6-B196). A
/// Rule may be changed and moved in one set, but each only once, and a Rule
/// the set deletes can't be changed or moved by it too.
#[derive(Default)]
struct RuleTouches {
    edited: HashSet<RuleId>,
    moved: HashSet<RuleId>,
    deleted: HashSet<RuleId>,
}

impl RuleTouches {
    fn add(&mut self, id: RuleId, touch: RuleTouch) -> Result<(), ToolError> {
        let also_deleted = match touch {
            RuleTouch::Delete => self.edited.contains(&id) || self.moved.contains(&id),
            RuleTouch::Edit | RuleTouch::Move => self.deleted.contains(&id),
        };
        if also_deleted {
            return Err(ToolError::NotChangeable {
                argument: "rule_id",
                reason: "also-deleted",
            });
        }
        let own = match touch {
            RuleTouch::Edit => &mut self.edited,
            RuleTouch::Move => &mut self.moved,
            RuleTouch::Delete => &mut self.deleted,
        };
        if !own.insert(id) {
            return Err(ToolError::BadArgument {
                argument: "rule_id",
            });
        }
        Ok(())
    }
}

/// A Rule's name as the book compares names: trimmed, ignoring capitals.
fn rule_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn collect_rules(nodes: &[RuleNode], parent: Option<RuleId>, into: &mut HashMap<RuleId, RuleInfo>) {
    for node in nodes {
        into.insert(
            node.rule.id,
            RuleInfo {
                rule: node.rule.clone(),
                parent,
                sub_rules: node.sub_rules.len(),
            },
        );
        collect_rules(&node.sub_rules, Some(node.rule.id), into);
    }
}

impl Known {
    fn read(book: &Book) -> Result<Self, StoreError> {
        let accounts: HashMap<_, _> = book
            .all_accounts()?
            .into_iter()
            .map(|account| (account.id, account))
            .collect();
        let paths: HashMap<AccountId, Vec<String>> = book
            .categories()?
            .into_iter()
            .map(|entry| (entry.category.id, entry.path))
            .collect();
        let mut currencies = accounts
            .values()
            .filter(|account| account.kind.is_account() && !account.closed)
            .map(|account| account.currency);
        let usual_currency = currencies
            .next()
            .filter(|first| currencies.all(|other| other == *first));
        let tree = book.rules()?;
        let mut rules = HashMap::new();
        collect_rules(&tree, None, &mut rules);
        let mut rule_names: HashMap<Option<RuleId>, HashSet<String>> = HashMap::new();
        for info in rules.values() {
            rule_names
                .entry(info.parent)
                .or_default()
                .insert(rule_name_key(&info.rule.name));
        }
        Ok(Self {
            accounts,
            original_paths: paths.clone(),
            paths,
            keys: HashMap::new(),
            usual_currency,
            rules,
            rule_order: tree.iter().map(|node| node.rule.id).collect(),
            rule_names,
            rule_keys: HashMap::new(),
            deleted_rules: HashSet::new(),
        })
    }

    /// Works out the path of every category again, after a rename or a move.
    fn refresh_paths(&mut self) {
        let mut paths = HashMap::new();
        for category in self.accounts.values().filter(|a| a.kind.is_category()) {
            let mut names = vec![category.name.clone()];
            let mut above = category.parent;
            // A loop can't be in a book that was read, and a move into itself
            // is refused, but a step limit keeps this safe regardless.
            let mut steps = 0;
            while let Some(id) = above {
                let Some(parent) = self.accounts.get(&id) else {
                    break;
                };
                names.push(parent.name.clone());
                above = parent.parent;
                steps += 1;
                if steps > self.accounts.len() {
                    break;
                }
            }
            names.reverse();
            paths.insert(category.id, names);
        }
        self.paths = paths;
    }

    /// The path of the category a Rule books to, if it books to one.
    fn rule_category(&self, rule: &Rule) -> Option<Vec<String>> {
        match rule.actions.book_to {
            Some(BookTo::Category(id)) => Some(self.path(id)),
            _ => None,
        }
    }

    /// An open category the set may book to: one of the book's, by
    /// `category_id`, or a new one, by `new_category`.
    fn target(&self, item: &Object<'_>) -> Result<AccountId, ToolError> {
        match (item.id("category_id")?, item.string("new_category")?) {
            (Some(id), None) => {
                let usable = self
                    .accounts
                    .get(&id)
                    .is_some_and(|category| category.kind.is_category() && !category.closed);
                if usable {
                    Ok(id)
                } else {
                    Err(ToolError::UnknownCategory)
                }
            }
            (None, Some(key)) => self.keys.get(key).copied().ok_or(ToolError::BadArgument {
                argument: "new_category",
            }),
            _ => Err(ToolError::BadArgument {
                argument: "category_id",
            }),
        }
    }

    fn path(&self, id: AccountId) -> Vec<String> {
        self.paths.get(&id).cloned().unwrap_or_default()
    }

    fn new_category(&mut self, value: &Value) -> Result<NewCategory, ToolError> {
        let item = Object::read(
            value,
            &["key", "name", "parent_id", "income", "currency"],
            "new_categories",
        )?;
        let key = item.required_string("key")?.trim();
        if key.is_empty() || key.chars().count() > MAX_KEY_CHARS || self.keys.contains_key(key) {
            return Err(ToolError::BadArgument { argument: "key" });
        }
        let name = plain_text(item.required_string("name")?, "name")?;
        let parent = match item.id("parent_id")? {
            Some(id) => Some(
                self.accounts
                    .get(&id)
                    .filter(|parent| parent.kind.is_category() && !parent.closed)
                    .ok_or(ToolError::UnknownCategory)?
                    .clone(),
            ),
            None => None,
        };
        // The kind is never taken from the parent silently: a model that
        // leaves `income` out asked for a spending category (the default),
        // and one inside an income category is refused, so that a new
        // spending category cannot end up under an income group.
        let income = item.boolean("income")?.unwrap_or(false);
        if let Some(parent) = &parent
            && (parent.kind == AccountKind::Income) != income
        {
            return Err(ToolError::BadArgument { argument: "income" });
        }
        let asked_currency = item
            .string("currency")?
            .map(|code| {
                CurrencyCode::new(code)
                    .ok()
                    .filter(|currency| {
                        currency.iso_minor_digits().is_some() && currency.withdrawn().is_none()
                    })
                    .ok_or(ToolError::BadArgument {
                        argument: "currency",
                    })
            })
            .transpose()?;
        let currency = match (&parent, asked_currency) {
            (Some(parent), Some(asked)) if asked != parent.currency => {
                return Err(ToolError::BadArgument {
                    argument: "currency",
                });
            }
            (Some(parent), _) => parent.currency,
            (None, Some(asked)) => asked,
            (None, None) => self.usual_currency.ok_or(ToolError::BadArgument {
                argument: "currency",
            })?,
        };
        let kind = if income {
            AccountKind::Income
        } else {
            AccountKind::Expense
        };
        let parent_id = parent.as_ref().map(|parent| parent.id);
        // Categories of one kind inside the same category have different
        // names, as the store checks when it adds one.
        let wanted = name.to_lowercase();
        if self.accounts.values().any(|other| {
            other.kind == kind
                && other.parent == parent_id
                && other.name.trim().to_lowercase() == wanted
        }) {
            return Err(ToolError::NameTaken);
        }
        let id = AccountId::generate();
        let mut path = parent_id
            .map(|parent| self.path(parent))
            .unwrap_or_default();
        path.push(name.clone());
        self.accounts.insert(
            id,
            Account {
                id,
                name,
                kind,
                currency,
                account_type: None,
                parent: parent_id,
                closed: false,
                number: None,
                notes: None,
                institution: None,
            },
        );
        self.paths.insert(id, path.clone());
        self.keys.insert(key.to_owned(), id);
        Ok(NewCategory {
            id,
            path,
            parent: parent_id,
            income,
            currency,
        })
    }

    /// Renames or moves a category of the book, or both.
    fn edit_category(&mut self, value: &Value) -> Result<EditCategory, ToolError> {
        let item = Object::read(
            value,
            &["category_id", "name", "parent_id", "new_parent", "to_top"],
            "edit_categories",
        )?;
        let id = item.id("category_id")?.ok_or(ToolError::BadArgument {
            argument: "category_id",
        })?;
        let reviewed = self
            .accounts
            .get(&id)
            .filter(|category| category.kind.is_category() && !category.closed)
            .ok_or(ToolError::UnknownCategory)?
            .clone();
        // A category made by this set isn't in the book to edit.
        if self.keys.values().any(|&new| new == id) {
            return Err(ToolError::UnknownCategory);
        }
        let name = item
            .string("name")?
            .map(|name| plain_text(name, "name"))
            .transpose()?
            .filter(|name| *name != reviewed.name);
        let to_top = item.boolean("to_top")?.unwrap_or(false);
        let parent = match (item.id("parent_id")?, item.string("new_parent")?, to_top) {
            (None, None, false) => None,
            (None, None, true) => Some(None),
            (Some(parent), None, false) => Some(Some(parent)),
            (None, Some(key), false) => Some(Some(self.keys.get(key).copied().ok_or(
                ToolError::BadArgument {
                    argument: "new_parent",
                },
            )?)),
            _ => {
                return Err(ToolError::BadArgument {
                    argument: "parent_id",
                });
            }
        };
        if let Some(Some(parent_id)) = parent {
            let parent = self
                .accounts
                .get(&parent_id)
                .filter(|parent| parent.kind.is_category() && !parent.closed)
                .ok_or(ToolError::UnknownCategory)?;
            // Same kind and currency as the category, and not inside itself
            // or one of its own subcategories.
            if parent.kind != reviewed.kind || parent.currency != reviewed.currency {
                return Err(ToolError::BadArgument {
                    argument: "parent_id",
                });
            }
            let mut above = Some(parent_id);
            let mut steps = 0;
            while let Some(next) = above {
                if next == id || steps > self.accounts.len() {
                    return Err(ToolError::BadArgument {
                        argument: "parent_id",
                    });
                }
                steps += 1;
                above = self.accounts.get(&next).and_then(|a| a.parent);
            }
        }
        let parent = parent.filter(|parent| *parent != reviewed.parent);
        if name.is_none() && parent.is_none() {
            return Err(ToolError::BadArgument {
                argument: "category_id",
            });
        }
        let new_name = name.clone().unwrap_or_else(|| reviewed.name.clone());
        let new_parent = parent.unwrap_or(reviewed.parent);
        let wanted = new_name.to_lowercase();
        if self.accounts.values().any(|other| {
            other.id != id
                && other.kind == reviewed.kind
                && other.parent == new_parent
                && other.name.trim().to_lowercase() == wanted
        }) {
            return Err(ToolError::NameTaken);
        }
        let before = self.original_paths.get(&id).cloned().unwrap_or_default();
        if let Some(category) = self.accounts.get_mut(&id) {
            category.name = new_name;
            category.parent = new_parent;
        }
        self.refresh_paths();
        Ok(EditCategory {
            id,
            before,
            after: self.path(id),
            name,
            parent,
            reviewed,
        })
    }

    /// A Rule of the book that the set doesn't touch yet.
    fn rule_to_change(
        &self,
        item: &Object<'_>,
        touched: &mut RuleTouches,
        touch: RuleTouch,
    ) -> Result<&RuleInfo, ToolError> {
        let id = item
            .uuid("rule_id")?
            .map(RuleId::from_bytes)
            .ok_or(ToolError::BadArgument {
                argument: "rule_id",
            })?;
        let info = self.rules.get(&id).ok_or(ToolError::UnknownRule)?;
        touched.add(id, touch)?;
        Ok(info)
    }

    /// Changes an existing Rule: the name, whether all or any condition
    /// counts, the conditions and the category it books to. What isn't named
    /// stays.
    fn edit_rule(
        &mut self,
        value: &Value,
        touched: &mut RuleTouches,
    ) -> Result<EditRule, ToolError> {
        let item = Object::read(
            value,
            &[
                "rule_id",
                "name",
                "match",
                "conditions",
                "category_id",
                "new_category",
            ],
            "edit_rules",
        )?;
        let info = self.rule_to_change(&item, touched, RuleTouch::Edit)?;
        let before = info.rule.clone();
        let parent = info.parent;
        let mut rule = before.clone();
        if let Some(name) = item.string("name")? {
            let name = plain_text(name, "name")?;
            // Its old name is free for the next Rule of the set, its new
            // one is taken.
            let old = rule_name_key(&before.name);
            let wanted = rule_name_key(&name);
            if wanted != old {
                let names = self.rule_names.entry(parent).or_default();
                if names.contains(&wanted) {
                    return Err(ToolError::NameTaken);
                }
                names.remove(&old);
                names.insert(wanted);
            }
            rule.name = name;
        }
        match item.string("match")? {
            None => {}
            Some("all") => rule.match_mode = MatchMode::All,
            Some("any") => rule.match_mode = MatchMode::Any,
            Some(_) => return Err(ToolError::BadArgument { argument: "match" }),
        }
        if item.get("conditions").is_some() {
            // A Rule with an IBAN, a date, an amount or an account in it, or
            // a text list_rules shows cut, has conditions a model can't write
            // back, so it couldn't change them without dropping those
            // (list_rules says `conditions_complete: false`, M6-B200).
            if !before.conditions.iter().all(writable) {
                return Err(ToolError::NotChangeable {
                    argument: "conditions",
                    reason: "conditions-hidden",
                });
            }
            rule.conditions = conditions(item.array("conditions")?)?;
        }
        if item.get("category_id").is_some() || item.get("new_category").is_some() {
            if matches!(before.actions.book_to, Some(BookTo::Transfer(_))) {
                return Err(ToolError::NotChangeable {
                    argument: "category_id",
                    reason: "makes-transfer",
                });
            }
            rule.actions.book_to = Some(BookTo::Category(self.target(&item)?));
        }
        if rule == before {
            return Err(ToolError::BadArgument {
                argument: "rule_id",
            });
        }
        rule.check().map_err(rule_refused)?;
        Ok(EditRule {
            before_category: self.rule_category(&before),
            category: self.rule_category(&rule),
            before,
            rule,
        })
    }

    /// Deletes a Rule that has no sub-Rules.
    fn delete_rule(
        &mut self,
        value: &Value,
        touched: &mut RuleTouches,
    ) -> Result<DeleteRule, ToolError> {
        let item = Object::read(value, &["rule_id"], "delete_rules")?;
        let info = self.rule_to_change(&item, touched, RuleTouch::Delete)?;
        if info.sub_rules > 0 {
            return Err(ToolError::NotChangeable {
                argument: "rule_id",
                reason: "has-sub-rules",
            });
        }
        let rule = info.rule.clone();
        let parent = info.parent;
        self.deleted_rules.insert(rule.id);
        self.rule_order.retain(|id| *id != rule.id);
        // Its name is free for a new Rule of the same call.
        if let Some(names) = self.rule_names.get_mut(&parent) {
            names.remove(&rule_name_key(&rule.name));
        }
        Ok(DeleteRule {
            category: self.rule_category(&rule),
            rule,
        })
    }

    /// Moves a top-level Rule to a place in the list as it stands after the
    /// changes before it in the set.
    fn move_rule(
        &mut self,
        value: &Value,
        touched: &mut RuleTouches,
    ) -> Result<MoveRule, ToolError> {
        let item = Object::read(value, &["rule_id", "position"], "move_rules")?;
        let info = self.rule_to_change(&item, touched, RuleTouch::Move)?;
        if info.parent.is_some() {
            return Err(ToolError::NotChangeable {
                argument: "rule_id",
                reason: "is-sub-rule",
            });
        }
        let rule = info.rule.clone();
        let to = item
            .get("position")
            .and_then(Value::as_u64)
            .and_then(|position| usize::try_from(position).ok())
            .filter(|position| (1..=self.rule_order.len()).contains(position))
            .ok_or(ToolError::BadArgument {
                argument: "position",
            })?;
        let from = self
            .rule_order
            .iter()
            .position(|id| *id == rule.id)
            .ok_or(ToolError::UnknownRule)?
            + 1;
        if from == to {
            return Err(ToolError::BadArgument {
                argument: "position",
            });
        }
        self.rule_order.remove(from - 1);
        self.rule_order.insert(to - 1, rule.id);
        Ok(MoveRule {
            id: rule.id,
            name: rule.name,
            from,
            to,
        })
    }

    fn new_rule(&mut self, value: &Value) -> Result<NewRule, ToolError> {
        let item = Object::read(
            value,
            &[
                "key",
                "name",
                "parent_rule_id",
                "new_parent_rule",
                "match",
                "conditions",
                "category_id",
                "new_category",
            ],
            "new_rules",
        )?;
        let key = match item.string("key")? {
            None => None,
            Some(key) => {
                let key = key.trim();
                if key.is_empty()
                    || key.chars().count() > MAX_KEY_CHARS
                    || self.rule_keys.contains_key(key)
                {
                    return Err(ToolError::BadArgument { argument: "key" });
                }
                Some(key.to_owned())
            }
        };
        let name = plain_text(item.required_string("name")?, "name")?;
        // The Rule it sits inside, if any: one of the book's, or one earlier
        // in this list. Not one the set deletes, and not too deep.
        let (parent, inside, depth) = match (
            item.uuid("parent_rule_id")?.map(RuleId::from_bytes),
            item.string("new_parent_rule")?,
        ) {
            (None, None) => (None, None, 1),
            (Some(_), Some(_)) => {
                return Err(ToolError::BadArgument {
                    argument: "parent_rule_id",
                });
            }
            (Some(id), None) => {
                if self.deleted_rules.contains(&id) {
                    return Err(ToolError::UnknownRule);
                }
                let info = self.rules.get(&id).ok_or(ToolError::UnknownRule)?;
                let mut depth = 1;
                let mut above = info.parent;
                while let Some(rule) = above {
                    depth += 1;
                    above = self.rules.get(&rule).and_then(|info| info.parent);
                }
                (Some(id), Some(info.rule.name.clone()), depth + 1)
            }
            (None, Some(parent_key)) => {
                let (id, parent_name, depth) =
                    self.rule_keys
                        .get(parent_key.trim())
                        .ok_or(ToolError::BadArgument {
                            argument: "new_parent_rule",
                        })?;
                (Some(*id), Some(parent_name.clone()), depth + 1)
            }
        };
        if depth > MAX_RULE_DEPTH {
            return Err(ToolError::BadArgument {
                argument: if item.get("new_parent_rule").is_some() {
                    "new_parent_rule"
                } else {
                    "parent_rule_id"
                },
            });
        }
        // Against the Rules next to it as the changes and deletions before
        // it leave them, not only as the book has them now (M6-B191).
        if !self
            .rule_names
            .entry(parent)
            .or_default()
            .insert(rule_name_key(&name))
        {
            return Err(ToolError::NameTaken);
        }
        let match_mode = match item.string("match")? {
            None | Some("all") => MatchMode::All,
            Some("any") => MatchMode::Any,
            Some(_) => return Err(ToolError::BadArgument { argument: "match" }),
        };
        let conditions = conditions(item.array("conditions")?)?;
        let category = self.target(&item)?;
        let rule = Rule {
            id: RuleId::generate(),
            name,
            match_mode,
            conditions,
            actions: Actions {
                book_to: Some(BookTo::Category(category)),
                payee: None,
            },
        };
        rule.check().map_err(rule_refused)?;
        if let Some(key) = key {
            self.rule_keys
                .insert(key, (rule.id, rule.name.clone(), depth));
        }
        Ok(NewRule {
            category: self.path(category),
            rule,
            parent,
            inside,
        })
    }

    fn categorize(&self, book: &Book, value: &Value) -> Result<Categorize, ToolError> {
        let item = Object::read(
            value,
            &["transaction_id", "category_id", "new_category"],
            "categorize",
        )?;
        let id = item
            .uuid("transaction_id")?
            .map(TransactionId::from_bytes)
            .ok_or(ToolError::BadArgument {
                argument: "transaction_id",
            })?;
        let transaction = match book.transaction(id) {
            Err(StoreError::UnknownTransaction(_)) => return Err(ToolError::UnknownTransaction),
            other => other?,
        };
        let category = self.target(&item)?;
        // One part in one of the user's accounts, and one part booked to a
        // category or to none yet: anything else, such as a transfer, a
        // starting balance or a transaction split over several categories,
        // is the user's to change.
        let account_of = |split: &Split| split.account.and_then(|id| self.accounts.get(&id));
        let [first, second] = transaction.splits() else {
            return Err(ToolError::NotCategorizable);
        };
        let (own, part) = match (account_of(first), account_of(second)) {
            (Some(account), other) if account.kind.is_account() => (first, (second, other)),
            (other, Some(account)) if account.kind.is_account() => (second, (first, other)),
            _ => return Err(ToolError::NotCategorizable),
        };
        let (part, before) = part;
        let before = match before {
            None => None,
            Some(account) if account.kind.is_category() && !account.closed => Some(account),
            Some(_) => return Err(ToolError::NotCategorizable),
        };
        if before.is_some_and(|before| before.id == category) {
            return Err(ToolError::BadArgument {
                argument: "category_id",
            });
        }
        let target = &self.accounts[&category];
        if target.currency != transaction.currency() {
            return Err(ToolError::OtherCurrency);
        }
        let own_account = account_of(own).ok_or(ToolError::NotCategorizable)?;
        Ok(Categorize {
            transaction: id,
            date: transaction.date_of(own),
            payee: transaction.payee_of(own).map(str::to_owned),
            account: own_account.name.clone(),
            amount: own.account_amount(),
            before: before.map(|before| self.path(before.id)),
            after: self.path(category),
            category,
            part: part.id,
            reviewed: transaction,
        })
    }
}

/// Whether a model can write the condition back in a call: a text test or
/// a pattern on the payee, the other party's name or the bank's text, or
/// money in or out, whose text `list_rules` shows exactly as it is stored.
/// A text `list_rules` had to cut, or in which it left out an IBAN, would
/// come back changed (M6-B200).
pub(crate) fn writable(condition: &Condition) -> bool {
    match condition {
        Condition::Text { field, value, .. } => {
            *field != TextField::OtherPartyIban && for_model(value) == *value
        }
        Condition::Amount(test) => matches!(test, AmountTest::MoneyIn | AmountTest::MoneyOut),
        _ => false,
    }
}

/// The conditions of a Rule as a model wrote them: at least one, at most
/// [`MAX_CONDITIONS`]. An error names the condition by its place in the
/// list (M6-B191, D-626).
fn conditions(listed: &[Value]) -> Result<Vec<Condition>, ToolError> {
    let bad = |condition, reason| ToolError::BadCondition { condition, reason };
    if listed.is_empty() {
        return Err(bad(None, "none"));
    }
    if listed.len() > MAX_CONDITIONS {
        return Err(bad(None, "too-many"));
    }
    listed
        .iter()
        .enumerate()
        .map(|(index, value)| condition(value).map_err(|reason| bad(Some(index + 1), reason)))
        .collect()
}

/// One condition of a new Rule: a text test or a pattern on the payee, the
/// other party's name or the bank's text, or money in or out. No IBAN, since
/// a model never sees one (D-186). A pattern is the `regex` crate's, which
/// cannot hang (D-015; US1.6, D-627). The error is the fixed reason, as
/// `ai-tools.bad-condition` lists it.
fn condition(value: &Value) -> Result<Condition, &'static str> {
    let item = Object::read(
        value,
        &[
            "field",
            "test",
            "text",
            "case_sensitive",
            "money_in",
            "money_out",
        ],
        "conditions",
    )
    .map_err(|_| "shape")?;
    let money_in = item.boolean("money_in").map_err(|_| "shape")?;
    let money_out = item.boolean("money_out").map_err(|_| "shape")?;
    let field = item.string("field").map_err(|_| "field")?;
    match (field, money_in, money_out) {
        (None, Some(true), None) => Ok(Condition::Amount(AmountTest::MoneyIn)),
        (None, None, Some(true)) => Ok(Condition::Amount(AmountTest::MoneyOut)),
        (Some(field), None, None) => {
            let field = match field {
                "payee" => TextField::Payee,
                "other_party_name" => TextField::OtherPartyName,
                "bank_text" => TextField::RemittanceInformation,
                _ => return Err("field"),
            };
            let operator = match item.required_string("test").map_err(|_| "test")? {
                "contains" => TextOperator::Contains,
                "equals" => TextOperator::Equals,
                "starts_with" => TextOperator::StartsWith,
                "ends_with" => TextOperator::EndsWith,
                "matches_pattern" => TextOperator::Matches,
                _ => return Err("test"),
            };
            let text = item.required_string("text").map_err(|_| "text")?;
            Ok(Condition::Text {
                field,
                operator,
                value: if operator == TextOperator::Matches {
                    pattern_text(text)
                } else {
                    plain_text(text, "text")
                }
                .map_err(|_| "text")?,
                case_sensitive: item
                    .boolean("case_sensitive")
                    .map_err(|_| "shape")?
                    .unwrap_or(false),
            })
        }
        _ => Err("shape"),
    }
}

/// What a Rule the store would refuse tells the model: which condition and
/// why, from the Rule's own check.
fn rule_refused(error: RuleError) -> ToolError {
    let (condition, reason) = match error {
        RuleError::NoConditions(_) => (None, "none"),
        RuleError::EmptyText { condition, .. } => (Some(condition + 1), "text"),
        RuleError::BadPattern { condition, .. } => (Some(condition + 1), "bad-pattern"),
        RuleError::AmountRangeReversed { condition, .. }
        | RuleError::AmountRangeCurrencies { condition, .. }
        | RuleError::DateRangeReversed { condition, .. } => (Some(condition + 1), "invalid"),
        RuleError::EmptyPayee(_) => (None, "invalid"),
    };
    ToolError::BadCondition { condition, reason }
}

/// A name or text a model wrote, as the book will keep it: trimmed, not
/// empty, not too long, and without control characters or characters that
/// hide text (threat T-08).
fn plain_text(text: &str, argument: &'static str) -> Result<String, ToolError> {
    let text = text.trim();
    let bad = text.is_empty()
        || text.chars().count() > MAX_NAME_CHARS
        || text.chars().any(|c| c.is_control() || hides_text(c));
    if bad {
        return Err(ToolError::BadArgument { argument });
    }
    Ok(text.to_owned())
}

/// A pattern a model wrote, as the book will keep it: as written, not
/// empty, not too long, and without control characters or characters that
/// hide text (threat T-08). It is compiled by the Rule's own check, with the
/// linear-time `regex` crate (D-015), so no pattern can hang the app.
fn pattern_text(text: &str) -> Result<String, ToolError> {
    let bad = text.trim().is_empty()
        || text.chars().count() > MAX_PATTERN_CHARS
        || text.chars().any(|c| c.is_control() || hides_text(c));
    if bad {
        return Err(ToolError::BadArgument { argument: "text" });
    }
    Ok(text.to_owned())
}

/// A JSON object of a call that has only the keys it may have.
struct Object<'a> {
    values: &'a Map<String, Value>,
}

impl<'a> Object<'a> {
    /// `value` as an object with only `keys`; `argument` names it when it
    /// isn't one, or when it has another key, which the answer doesn't
    /// repeat but lists `keys` for (M6-B201, X-8).
    fn read(
        value: &'a Value,
        keys: &'static [&'static str],
        argument: &'static str,
    ) -> Result<Self, ToolError> {
        let values = value
            .as_object()
            .ok_or(ToolError::BadArgument { argument })?;
        if !values.keys().all(|key| keys.contains(&key.as_str())) {
            return Err(ToolError::UnknownField {
                argument,
                allowed: keys,
            });
        }
        Ok(Self { values })
    }

    /// The value `name`, if given; null counts as not given.
    fn get(&self, name: &str) -> Option<&'a Value> {
        self.values.get(name).filter(|value| !value.is_null())
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

    fn required_string(&self, name: &'static str) -> Result<&'a str, ToolError> {
        self.string(name)?
            .ok_or(ToolError::BadArgument { argument: name })
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

    /// A list; not given is an empty one.
    fn array(&self, name: &'static str) -> Result<&'a [Value], ToolError> {
        match self.get(name) {
            None => Ok(&[]),
            Some(Value::Array(items)) => Ok(items),
            Some(_) => Err(ToolError::BadArgument { argument: name }),
        }
    }

    fn uuid(&self, name: &'static str) -> Result<Option<[u8; 16]>, ToolError> {
        self.string(name)?
            .map(|text| {
                uuid::Uuid::try_parse(text)
                    .map(uuid::Uuid::into_bytes)
                    .map_err(|_| ToolError::BadArgument { argument: name })
            })
            .transpose()
    }

    fn id(&self, name: &'static str) -> Result<Option<AccountId>, ToolError> {
        Ok(self.uuid(name)?.map(AccountId::from_bytes))
    }
}

#[cfg(test)]
mod tests;
