// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The function that lets a model read the debug log of the developer mode
//! (D-173, D-227, threat T-33): only while AI and the developer mode are
//! both on, and only its fixed values, event names, details from a fixed
//! list and error codes.

use kassensturz_core::debug_log::DebugLog;
use serde_json::{Map, Value, json};

use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};

/// The most entries one call returns, and the default.
pub const MAX_LOG_ENTRIES: usize = 200;

/// The function that reads the debug log. The chat and the MCP server offer
/// it only while the developer mode is on (D-227).
pub const READ_DEBUG_LOG: Function = Function {
    name: "read_debug_log",
    description: "Reads the newest entries of KassenSturz's debug log, oldest first, which \
        is written while KassenSturz runs in its developer mode, to help find problems: when something happened (UTC), the event's \
        name, e.g. chat.function-called, the function, tool or screen it concerns, and the \
        error code when something failed. It holds no amounts, names or other text from the \
        book. Answers the error ai-tools.debug-log-off when the developer mode has ended.",
    parameters: r#"{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":200,"description":"Most entries to return, the newest ones. Default 200."}},"additionalProperties":false}"#,
};

/// Reads the newest entries of `log` for a model, with `arguments` as the
/// model sent them; `ai-tools.debug-log-off` unless AI and the log are both
/// on.
pub fn read_debug_log(log: &DebugLog, arguments: &Value) -> Result<Output, ToolError> {
    let limit = match arguments {
        Value::Null => None,
        Value::Object(values) => {
            if values.keys().any(|key| key != "limit") {
                return Err(ToolError::BadArgument {
                    argument: "arguments",
                });
            }
            values.get("limit").filter(|value| !value.is_null())
        }
        _ => {
            return Err(ToolError::BadArgument {
                argument: "arguments",
            });
        }
    };
    let limit = match limit {
        None => MAX_LOG_ENTRIES,
        Some(value) => value
            .as_u64()
            .and_then(|limit| usize::try_from(limit).ok())
            .filter(|limit| (1..=MAX_LOG_ENTRIES).contains(limit))
            .ok_or(ToolError::BadArgument { argument: "limit" })?,
    };
    if !log.readable_by_ai() {
        return Err(ToolError::DebugLogOff);
    }
    let mut figures = Figures::new();
    let entries: Vec<Value> = log
        .read(limit)
        .into_iter()
        .map(|entry| {
            let at = entry.at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string();
            figures.add_text(&at);
            let mut value = Map::new();
            value.insert("at".into(), Value::from(at));
            value.insert("event".into(), Value::from(entry.event.name()));
            if let Some(detail) = entry.detail {
                value.insert("detail".into(), Value::from(detail));
            }
            if let Some(code) = entry.code {
                value.insert("code".into(), Value::from(code));
            }
            Value::Object(value)
        })
        .collect();
    figures.add_count(entries.len() as u64);
    Ok(Output::new(
        json!({ "entries": entries, "count": entries.len() }),
        figures,
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use kassensturz_core::debug_log::Event;

    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "kassensturz-ai-tools-log-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn log(&self, on: bool, settings: &str) -> DebugLog {
            fs::write(self.0.join("kassensturzrc"), settings).unwrap();
            DebugLog::at(self.0.join("debug-log"), on, self.0.join("kassensturzrc"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const AI_ON: &str = "[AI]\nEnabled=true\n";

    #[test]
    fn the_log_is_read_only_while_ai_and_the_log_are_on() {
        let scratch = Scratch::new("switches");
        let log = scratch.log(true, AI_ON);
        log.record(Event::ChatFailed, None, Some("ai.timeout"));
        for (on, settings) in [
            (false, ""),
            (false, AI_ON),
            (true, ""),
            (true, "[AI]\nEnabled=false\n"),
        ] {
            let error = read_debug_log(&scratch.log(on, settings), &Value::Null).unwrap_err();
            assert!(matches!(error, ToolError::DebugLogOff), "{on} {settings:?}");
        }
        let output = read_debug_log(&scratch.log(true, AI_ON), &Value::Null).unwrap();
        assert_eq!(output.value["count"], 1);
        assert_eq!(output.value["entries"][0]["event"], "chat.failed");
        assert_eq!(output.value["entries"][0]["code"], "ai.timeout");
        assert!(output.value["entries"][0].get("detail").is_none());
    }

    #[test]
    fn the_limit_takes_the_newest_entries() {
        let scratch = Scratch::new("limit");
        let log = scratch.log(true, AI_ON);
        log.record(Event::AppStarted, None, None);
        log.record(Event::ScreenShown, Some("OverviewPage"), None);
        let output = read_debug_log(&log, &json!({"limit": 1})).unwrap();
        assert_eq!(output.value["entries"][0]["detail"], "OverviewPage");
        for bad in [
            json!({"limit": 0}),
            json!({"limit": 201}),
            json!({"limit": "5"}),
        ] {
            let error = read_debug_log(&log, &bad).unwrap_err();
            assert!(matches!(
                error,
                ToolError::BadArgument { argument: "limit" }
            ));
        }
        let error = read_debug_log(&log, &json!({"path": "/etc"})).unwrap_err();
        assert!(matches!(
            error,
            ToolError::BadArgument {
                argument: "arguments"
            }
        ));
    }
}
