// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What some local servers leave in a reply's text that belongs elsewhere
//! (D-593): a model's thinking in `<think>` blocks, function calls the
//! server did not turn into calls (`<tool_call>` blocks, as Qwen models
//! write them), and arguments wrapped in a Markdown code fence. Pure
//! functions over text; nothing here runs a call. A call read from the
//! text goes through the same checks as a native one: the offered list,
//! the limits, the argument checks, and a change is only prepared for the
//! user to confirm.

use serde_json::{Map, Value};

/// The text without the model's thinking: every `<think>…</think>` block,
/// everything up to a closing `</think>` that has no opening one (some
/// servers drop the opening tag), and an opening block that never closes
/// (the reply was cut while thinking). The rest is trimmed.
pub fn without_thinking(text: &str) -> String {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";
    if !text.contains(OPEN) && !text.contains(CLOSE) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // A closing tag before any opening one ends thinking that started
    // before the text.
    if let Some(close) = rest.find(CLOSE)
        && rest.find(OPEN).is_none_or(|open| close < open)
    {
        rest = &rest[close + CLOSE.len()..];
    }
    while let Some(open) = rest.find(OPEN) {
        out.push_str(&rest[..open]);
        let after = &rest[open + OPEN.len()..];
        match after.find(CLOSE) {
            Some(close) => rest = &after[close + CLOSE.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_owned()
}

/// Arguments without one Markdown code fence around them
/// (```` ```json … ``` ````), as some models write them. Anything else is
/// returned as it is.
pub fn unfenced(arguments: &str) -> &str {
    let trimmed = arguments.trim();
    let Some(inner) = trimmed
        .strip_prefix("```")
        .and_then(|rest| rest.strip_suffix("```"))
    else {
        return arguments;
    };
    // The fence's language word, if any, ends at the first line break.
    let inner = match inner.find('\n') {
        Some(line_end) if !inner[..line_end].contains(['{', '[']) => &inner[line_end + 1..],
        _ => inner,
    };
    inner.trim()
}

/// A function call found in a reply's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextCall {
    /// The function's name, as the model wrote it.
    pub name: String,
    /// Its arguments as a JSON object's text.
    pub arguments: String,
}

/// The calls a reply's text holds as `<tool_call>…</tool_call>` blocks,
/// and the text around them, trimmed. `None` when the text holds no such
/// block, or when one of them can't be read: then nothing is taken from
/// the text and the chat stops as before (`ai-chat.tool-call-as-text`).
///
/// A block holds either JSON (`{"name": …, "arguments": {…}}`, with
/// `parameters` read like `arguments`) or Qwen's XML form
/// (`<function=name><parameter=key>value</parameter></function>`). An XML
/// value is text, except a whole number (as for a `limit`) and a JSON
/// object, list, `true` or `false`, which are read as such; a decimal
/// stays text, as KassenSturz's amounts are text (D-065).
pub fn text_calls(text: &str, most: usize) -> Option<(Vec<TextCall>, String)> {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";
    if !text.contains(OPEN) {
        return None;
    }
    let mut calls = Vec::new();
    let mut rest_text = String::new();
    let mut rest = text;
    while let Some(open) = rest.find(OPEN) {
        rest_text.push_str(&rest[..open]);
        let after = &rest[open + OPEN.len()..];
        let close = after.find(CLOSE)?;
        if calls.len() == most {
            return None;
        }
        calls.push(read_block(&after[..close])?);
        rest = &after[close + CLOSE.len()..];
    }
    rest_text.push_str(rest);
    Some((calls, rest_text.trim().to_owned()))
}

fn read_block(block: &str) -> Option<TextCall> {
    let block = unfenced(block).trim();
    if block.starts_with('{') {
        let value: Value = serde_json::from_str(block).ok()?;
        let name = value.get("name")?.as_str()?.to_owned();
        let arguments = match value.get("arguments").or_else(|| value.get("parameters")) {
            None | Some(Value::Null) => "{}".to_owned(),
            Some(Value::Object(arguments)) => serde_json::to_string(arguments).ok()?,
            Some(Value::String(arguments)) => unfenced(arguments).to_owned(),
            Some(_) => return None,
        };
        return Some(TextCall { name, arguments });
    }
    read_xml(block)
}

/// `<function=name>` followed by `<parameter=key>value</parameter>` pairs
/// and `</function>`, with only white space between them.
fn read_xml(block: &str) -> Option<TextCall> {
    let rest = block.strip_prefix("<function=")?;
    let name_end = rest.find('>')?;
    let name = rest[..name_end].trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let mut rest = rest[name_end + 1..].trim_start();
    let mut arguments = Map::new();
    loop {
        if let Some(after) = rest.strip_prefix("</function>") {
            if !after.trim().is_empty() {
                return None;
            }
            break;
        }
        let after = rest.strip_prefix("<parameter=")?;
        let key_end = after.find('>')?;
        let key = after[..key_end].trim();
        let value_and_rest = &after[key_end + 1..];
        let value_end = value_and_rest.find("</parameter>")?;
        let value = value_and_rest[..value_end].trim();
        if key.is_empty() || arguments.contains_key(key) {
            return None;
        }
        arguments.insert(key.to_owned(), xml_value(value));
        rest = value_and_rest[value_end + "</parameter>".len()..].trim_start();
    }
    Some(TextCall {
        name: name.to_owned(),
        arguments: serde_json::to_string(&arguments).ok()?,
    })
}

fn xml_value(value: &str) -> Value {
    let whole_number = !value.is_empty()
        && value.len() <= 18
        && value
            .strip_prefix('-')
            .unwrap_or(value)
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        && value != "-";
    let structured = value.starts_with('{') || value.starts_with('[');
    if (whole_number || structured || value == "true" || value == "false")
        && let Ok(parsed) = serde_json::from_str::<Value>(value)
    {
        return parsed;
    }
    Value::String(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_is_left_out() {
        assert_eq!(without_thinking("Hello"), "Hello");
        assert_eq!(
            without_thinking("<think>\nLet me look.\n</think>\nThe balance is 12.00."),
            "The balance is 12.00."
        );
        assert_eq!(
            without_thinking("Sure.<think>a</think> Then <think>b</think>done."),
            "Sure. Then done."
        );
        // Opening tag dropped by the server.
        assert_eq!(
            without_thinking("hmm 9,999.00\n</think>\n\nAnswer"),
            "Answer"
        );
        // Cut while thinking: nothing to show.
        assert_eq!(without_thinking("<think>still going 123"), "");
        // Thinking that mentions a call is not a call.
        assert_eq!(
            without_thinking("<think>I could write <tool_call> here</think>No."),
            "No."
        );
    }

    #[test]
    fn one_fence_is_taken_off() {
        assert_eq!(unfenced("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(unfenced("```{\"a\":1}```"), "{\"a\":1}");
        assert_eq!(unfenced("  ```\n{}\n```  "), "{}");
        assert_eq!(unfenced("{\"a\":\"```\"}"), "{\"a\":\"```\"}");
        assert_eq!(unfenced("{}"), "{}");
    }

    #[test]
    fn json_blocks_are_read_as_calls() {
        let text = "<tool_call>\n{\"name\": \"list_accounts\", \"arguments\": {}}\n</tool_call>\n\
                    <tool_call>{\"name\":\"find_transactions\",\"arguments\":{\"text\":\"Ice\",\"limit\":5}}</tool_call>";
        let (calls, rest) = text_calls(text, 32).unwrap();
        assert_eq!(rest, "");
        assert_eq!(
            calls,
            [
                TextCall {
                    name: "list_accounts".into(),
                    arguments: "{}".into()
                },
                TextCall {
                    name: "find_transactions".into(),
                    arguments: r#"{"limit":5,"text":"Ice"}"#.into()
                }
            ]
        );
        let (calls, rest) = text_calls(
            "Let me look.\n<tool_call>{\"name\":\"a\",\"arguments\":\"{\\\"x\\\":1}\"}</tool_call>",
            32,
        )
        .unwrap();
        assert_eq!(rest, "Let me look.");
        assert_eq!(calls[0].arguments, r#"{"x":1}"#);
    }

    #[test]
    fn xml_blocks_are_read_as_calls() {
        let text = "<tool_call>\n<function=find_transactions>\n<parameter=text>\nNorthern Ice\n</parameter>\n\
                    <parameter=limit>\n20\n</parameter>\n<parameter=min_amount>\n49.99\n</parameter>\n\
                    <parameter=skip_transfers>\ntrue\n</parameter>\n</function>\n</tool_call>";
        let (calls, _) = text_calls(text, 32).unwrap();
        assert_eq!(calls[0].name, "find_transactions");
        let arguments: Value = serde_json::from_str(&calls[0].arguments).unwrap();
        assert_eq!(
            arguments,
            serde_json::json!({"text": "Northern Ice", "limit": 20, "min_amount": "49.99", "skip_transfers": true})
        );
        let (calls, _) = text_calls(
            "<tool_call><function=list_accounts></function></tool_call>",
            32,
        )
        .unwrap();
        assert_eq!(calls[0].arguments, "{}");
    }

    #[test]
    fn a_block_that_cant_be_read_takes_nothing() {
        for text in [
            "no call here",
            "<tool_call>{\"name\":\"a\"",
            "<tool_call>not json</tool_call>",
            "<tool_call>{\"arguments\":{}}</tool_call>",
            "<tool_call>{\"name\":\"a\",\"arguments\":5}</tool_call>",
            "<tool_call>{\"name\":\"a\"}</tool_call><tool_call>broken</tool_call>",
            "<tool_call><function=a><parameter=x>1</parameter></tool_call>",
            "<tool_call><function=a><parameter=x>1</parameter><parameter=x>2</parameter></function></tool_call>",
            "<tool_call><function=a b></function></tool_call>",
        ] {
            assert_eq!(text_calls(text, 32), None, "{text}");
        }
        let three = "<tool_call>{\"name\":\"a\"}</tool_call>".repeat(3);
        assert_eq!(text_calls(&three, 2), None);
        assert_eq!(text_calls(&three, 3).unwrap().0.len(), 3);
    }
}
