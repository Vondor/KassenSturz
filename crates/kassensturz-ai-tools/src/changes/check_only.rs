// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A call that is only checked (US2.15, #595, D-674): the user's own AI tool
//! asks with `check_only: true` and hears what the call would prepare, such
//! as a budget amount KassenSturz computes with `based_on`, before anything
//! goes in front of the user. Nothing is sent to the app.

use kassensturz_store::Book;
use serde_json::{Map, Value, json};

use super::{Proposal, ProposedChange};
use crate::error::ToolError;
use crate::writer::Writer;

/// The argument that asks for a check only.
pub const CHECK_ONLY: &str = "check_only";

/// What the MCP server adds to the parameters of `propose_changes` and
/// `propose_view`.
pub const CHECK_ONLY_SCHEMA: &str = r#"{"type":"boolean","description":"true: only check the call and answer what it would prepare (how many changes of each list, the budget amounts KassenSturz computes, and which conditions remove_conditions would take away); nothing is sent to KassenSturz and the user sees nothing. Then call again without it to prepare the changes. Default false."}"#;

/// The call without `check_only`, and whether it asked for a check only.
/// A value that is not `true` or `false` is refused.
pub fn take_check_only(arguments: &Value) -> Result<(bool, Value), ToolError> {
    let mut arguments = arguments.clone();
    let Some(object) = arguments.as_object_mut() else {
        return Ok((false, arguments));
    };
    let check_only = match object.remove(CHECK_ONLY) {
        None => false,
        Some(Value::Bool(value)) => value,
        Some(_) => {
            return Err(ToolError::BadArgument {
                argument: CHECK_ONLY,
            });
        }
    };
    Ok((check_only, arguments))
}

/// The answer to a call that was only checked: what the call answers when
/// it prepares the set (its notes, such as an average's), with a status
/// that says nothing was sent, the count of the changes per list, and every
/// budget amount the set would plan, as exact decimal text, and the conditions
/// `remove_conditions` would take away, as `list_rules` shows them. No other
/// text of the book is in it, so nothing reaches the model that a read
/// function would have masked.
pub fn check_only_answer(book: &Book, proposal: &Proposal) -> Result<Value, ToolError> {
    let mut answer = match &proposal.output.value {
        Value::Object(object) => object.clone(),
        _ => Map::new(),
    };
    answer.remove("prepared");
    let set = &proposal.changes;
    let mut writer = Writer::new(book);
    let mut lists = Map::new();
    let mut amounts = Vec::new();
    for (slot, change) in set.slots().iter().zip(set.changes()) {
        let slot = slot.to_string();
        let (list, index) = slot.split_once('/').unwrap_or((slot.as_str(), "0"));
        let count = lists.entry(list.to_owned()).or_insert(json!(0));
        *count = json!(count.as_u64().unwrap_or(0) + 1);
        let item = index.parse::<usize>().unwrap_or(0) + 1;
        let (amount, from, basis) = match change {
            ProposedChange::NewBudget(new) => (new.amount, new.first_day, new.basis),
            ProposedChange::EditBudget(edit) if edit.changes_amount => {
                (edit.amount, edit.from_day, edit.basis)
            }
            _ => continue,
        };
        let mut row = json!({
            "list": list,
            "item": item,
            "amount": writer.amount(amount)?,
            "currency": amount.currency().as_str(),
            "from": from.to_string(),
        });
        // How KassenSturz computed it, for an amount `based_on` spending.
        if let Some(basis) = basis {
            row["based_on"] = json!({
                "periods": basis.periods,
                "from": basis.first_day.to_string(),
                "to": basis.last_day.to_string(),
                "spent": writer.amount(basis.spent)?,
                "average": writer.amount(basis.average)?,
                "rounded_up_to": basis.round_to,
            });
            // The empty newest period the count moved back past, as the
            // review card says it (M6-B209).
            if let Some((left_from, left_to)) = basis.left_out {
                row["based_on"]["left_out"] = json!({
                    "from": left_from.to_string(),
                    "to": left_to.to_string(),
                    "reason": "nothing-spent",
                });
            }
        }
        amounts.push(row);
    }
    let removed = removed_conditions(book, &mut writer, set.slots(), set.changes())?;
    if !removed.is_empty() {
        answer.insert("removed_conditions".into(), Value::Array(removed));
    }
    answer.insert("check_only".into(), json!(true));
    answer.insert(
        "status".into(),
        json!(
            "checked only: the call works, and nothing was sent to KassenSturz or shown to the \
             user; call again without check_only to prepare it"
        ),
    );
    answer.insert("would_prepare".into(), json!(set.changes().len()));
    answer.insert("lists".into(), Value::Object(lists));
    if !amounts.is_empty() {
        answer.insert("budget_amounts".into(), Value::Array(amounts));
    }
    Ok(Value::Object(answer))
}

/// The conditions `remove_conditions` would take away, one row per Rule
/// that loses some: the list and item of the call, and each condition by
/// its place and as `list_rules` shows it, with the same cutting and
/// masking, so a model can see it picked the right ones (US3.5, D-697).
/// The Rule's own read comes with the right to change it (D-252), so this
/// shows nothing `list_rules` would not.
fn removed_conditions(
    book: &Book,
    writer: &mut Writer<'_>,
    slots: &[impl core::fmt::Display],
    changes: &[ProposedChange],
) -> Result<Vec<Value>, ToolError> {
    let mut rows = Vec::new();
    let mut names = None;
    for (slot, change) in slots.iter().zip(changes) {
        let ProposedChange::EditRule(edit) = change else {
            continue;
        };
        if edit.removed_conditions.is_empty() {
            continue;
        }
        let names = match &mut names {
            Some(names) => names,
            none => none.insert(crate::read::Names::read(book)?),
        };
        let slot = slot.to_string();
        let (list, index) = slot.split_once('/').unwrap_or((slot.as_str(), "0"));
        let mut conditions = Vec::new();
        for &place in &edit.removed_conditions {
            let Some(condition) = edit.before.conditions.get(place - 1) else {
                continue;
            };
            let mut row = match crate::read::condition_row(writer, names, condition)? {
                Value::Object(row) => row,
                other => Map::from_iter([("condition".to_owned(), other)]),
            };
            row.insert("place".into(), json!(place));
            conditions.push(Value::Object(row));
        }
        rows.push(json!({
            "list": list,
            "item": index.parse::<usize>().unwrap_or(0) + 1,
            "conditions": conditions,
        }));
    }
    Ok(rows)
}
