// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What goes to a provider and what comes back: messages, the functions a
//! model may call, and its reply.
//!
//! Everything in a [`Reply`] comes from the provider and is untrusted
//! (threat T-21, X-8): its text is shown as plain text, and a function
//! call is only a request that the chat (M5.4) checks against the fixed
//! list of read functions (D-170). A call for a function that wasn't
//! offered is not handed on at all (threat T-31).

use serde_json::Value;

/// Who a message is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// KassenSturz's own instructions to the model.
    System,
    /// The user.
    User,
    /// The model.
    Assistant,
    /// The result of a function the model called.
    Tool,
}

impl Role {
    /// The role's name in the OpenAI-compatible API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// One message of a conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Who it is from.
    pub role: Role,
    /// Its text.
    pub content: String,
    /// The functions the model called in this message ([`Role::Assistant`]
    /// only).
    pub calls: Vec<ToolCall>,
    /// The call this message answers ([`Role::Tool`] only).
    pub call_id: Option<String>,
}

impl Message {
    fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            calls: Vec::new(),
            call_id: None,
        }
    }

    /// KassenSturz's instructions to the model.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    /// What the user wrote.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    /// What the model answered earlier, with the functions it called.
    pub fn assistant(content: impl Into<String>, calls: Vec<ToolCall>) -> Self {
        Self {
            calls,
            ..Self::new(Role::Assistant, content)
        }
    }

    /// The result of the call `call_id`.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            call_id: Some(call_id.into()),
            ..Self::new(Role::Tool, content)
        }
    }
}

/// A function the model may call: its name, what it does, and its
/// parameters as a JSON Schema object.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionSpec {
    /// The function's name.
    pub name: String,
    /// What it does, for the model.
    pub description: String,
    /// Its parameters, as a JSON Schema object.
    pub parameters: Value,
}

/// A function the model asked to call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    /// The provider's ID of the call, which the result names.
    pub id: String,
    /// The function, always one that was offered.
    pub name: String,
    /// The arguments as the model wrote them, a JSON text that the chat
    /// checks before it uses any of it.
    pub arguments: String,
}

/// Why the model stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Finish {
    /// It finished its answer.
    Done,
    /// It wants functions called.
    Calls,
    /// It hit the provider's length limit.
    Length,
    /// The provider filtered the answer, or gave another reason.
    Other,
}

/// What the model answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// Its text, possibly empty when it only calls functions.
    pub text: String,
    /// The functions it asked to call that were offered.
    pub calls: Vec<ToolCall>,
    /// The IDs of calls for functions that weren't offered. They are not
    /// carried out; the chat answers each with an error (threat T-31).
    pub ignored_calls: Vec<String>,
    /// Why it stopped.
    pub finish: Finish,
    /// How many tokens the provider counted in the request it answered,
    /// when it says (`usage.prompt_tokens`).
    pub prompt_tokens: Option<u64>,
}
