// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenario B7 of the outside security review: destroying the way
//! back. A manipulated AI must not reach the history's retention, "Clean up
//! history", the backups or the book folder, even with every cell of the
//! permission grid switched on and every field unlocked. All data is made
//! up (AGENTS.md §3.1 rule 2).

#![cfg(all(feature = "ai", target_os = "linux"))]

mod common;

use common::{Folder, Session};
use kassensturz_core::ai_switch::{Field, Permission};
use serde_json::{Value, json};

/// Words a tool or one of its arguments would carry if it reached the way
/// back.
const WAY_BACK: &[&str] = &[
    "history",
    "retention",
    "keep_days",
    "backup",
    "clean",
    "undo",
    "redo",
    "folder",
    "setting",
    "permission",
    "consent",
    "provider",
];

/// Every name in `schema`: the property names at every depth, and the
/// values of every `enum`.
fn names_in(schema: &Value, names: &mut Vec<String>) {
    match schema {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "properties"
                    && let Value::Object(properties) = value
                {
                    names.extend(properties.keys().cloned());
                }
                if key == "enum"
                    && let Value::Array(values) = value
                {
                    names.extend(values.iter().filter_map(Value::as_str).map(str::to_owned));
                }
                if key != "description" {
                    names_in(value, names);
                }
            }
        }
        Value::Array(values) => values.iter().for_each(|value| names_in(value, names)),
        _ => {}
    }
}

#[test]
fn b7_no_tool_reaches_the_history_the_backups_or_the_settings() {
    let folder = Folder::new("attack-way-back");
    let mut lines: Vec<String> = Permission::ALL
        .iter()
        .map(|permission| format!("{}=true", permission.key()))
        .collect();
    lines.extend(
        Field::ALL
            .iter()
            .map(|field| format!("{}=false", field.key())),
    );
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    folder.allow(&lines);
    let path = folder.book();
    let mut session = Session::start(folder.path(), &path);
    session.initialize("2025-06-18");
    let tools = session.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .expect("tools")
        .clone();
    assert!(
        tools.len() > 10,
        "the write tools are offered: {}",
        tools.len()
    );
    let mut names = Vec::new();
    for tool in &tools {
        names.push(tool["name"].as_str().expect("a name").to_owned());
        names_in(&tool["inputSchema"], &mut names);
    }
    let reaching: Vec<&String> = names
        .iter()
        .filter(|name| {
            let name = name.to_lowercase();
            WAY_BACK.iter().any(|word| name.contains(word))
        })
        .collect();
    assert!(
        reaching.is_empty(),
        "these reach the way back: {reaching:?}"
    );
    session.finish();
}
