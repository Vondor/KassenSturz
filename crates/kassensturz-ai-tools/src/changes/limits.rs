// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The limits per list that the parameters of `propose_changes` state
//! for each of its lists (`maxItems`), enforced in code (B-247, attack B1 of the security review).
//!
//! A model is told the limits in the JSON schema, but nothing stopped a call
//! that ignored them: 101 edits of transactions were prepared, so one
//! confirm took more than the review was made to show. The limits are read
//! from the schema the model is offered, so a list and its limit can't
//! drift apart.

use std::sync::LazyLock;

use serde_json::Value;

use crate::error::ToolError;

/// The parameters of `propose_changes`, read once.
static PARAMETERS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(super::PROPOSE_CHANGES.parameters).expect("the parameters are JSON")
});

/// Refuses `arguments` with [`ToolError::TooManyChanges`] when one of its
/// lists holds more items than the parameters of `propose_changes` allow
/// for it. A list the parameters don't name is left to the other checks,
/// and so are the lists inside an item (a Rule's conditions, a split's
/// parts, a transaction's tags) and `sort_with_rules`, whose own checks
/// already say more precisely what is wrong.
pub(crate) fn check(arguments: &Value) -> Result<(), ToolError> {
    let (Some(fields), Some(properties)) = (
        arguments.as_object(),
        PARAMETERS.get("properties").and_then(Value::as_object),
    ) else {
        return Ok(());
    };
    for (name, value) in fields {
        if name == super::sort_rules::LIST {
            continue;
        }
        let limit = properties
            .get(name)
            .and_then(|list| list.get("maxItems"))
            .and_then(Value::as_u64);
        let count = value.as_array().map_or(0, Vec::len);
        if limit.is_some_and(|limit| u64::try_from(count).unwrap_or(u64::MAX) > limit) {
            return Err(ToolError::TooManyChanges);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ids(count: usize) -> Vec<Value> {
        (0..count)
            .map(|n| json!({"transaction_id": n.to_string()}))
            .collect()
    }

    #[test]
    fn a_list_up_to_its_limit_passes() {
        assert!(check(&json!({"edit_transactions": ids(100)})).is_ok());
        assert!(check(&json!({"new_accounts": ids(10)})).is_ok());
    }

    #[test]
    fn a_list_over_its_limit_is_refused() {
        for (list, limit) in [
            ("edit_transactions", 100),
            ("categorize", 100),
            ("new_categories", 20),
            ("delete_transactions", 20),
            ("new_accounts", 10),
            ("join_transfers", 50),
        ] {
            assert!(
                matches!(
                    check(&json!({list: ids(limit + 1)})),
                    Err(ToolError::TooManyChanges)
                ),
                "{list} with {} items was let through",
                limit + 1
            );
        }
    }

    #[test]
    fn what_the_parameters_do_not_name_is_left_to_the_other_checks() {
        assert!(check(&json!({"new_views": ids(500)})).is_ok());
        assert!(check(&json!("not an object")).is_ok());
    }
}
