// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One conversation with the model: the loop of the read functions, the
//! fallback to the overview, the number check and the tool-call log (D-190).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use kassensturz_ai::{AiError, Finish, FunctionSpec, Message, PreparedRequest, Provider, ToolCall};
use kassensturz_ai_tools::{FUNCTIONS, Figures, ToolError, Unverified, check_numbers};
use kassensturz_core::{Date, ErrorParam, ErrorReport, Report};
use kassensturz_platform::Secret;
use kassensturz_store::Book;
use serde_json::{Map, Value, json};

use crate::error::ChatError;
use crate::log::{LogEntry, MAX_ARGUMENT_CHARS, cut};
use crate::prompt::{self, BuildInfo};

/// The most requests one question may take. A model that is still calling
/// functions after them gets `ai-chat.too-many-rounds`; a question about
/// the book needs two or three.
pub const MAX_ROUNDS: usize = 8;

/// What the model is told when it asked for a function that isn't offered.
/// It goes as a message of its own because a function's answer must name a
/// call the provider knows, and such a call was never handed on (threat
/// T-31, D-190).
pub const NOT_OFFERED: &str = "KassenSturz: a function you asked for is not offered, so it \
                               was not run (error ai-tools.unknown-function). Call only the \
                               functions offered.";

/// How the model gets the book's figures (D-052).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// It calls the read functions.
    Functions,
    /// It can't call functions and gets the small overview with each
    /// request.
    Overview,
}

/// What the model answered to a question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// The answer, untrusted text from the model: shown as plain text and
    /// labelled as AI (threat T-34).
    pub text: String,
    /// Every number in it that no function produced, to be marked (D-188,
    /// threat T-26).
    pub unverified: Vec<Unverified>,
    /// Why the model stopped: [`Finish::Length`] means the answer was cut.
    pub finish: Finish,
    /// How the model got its figures.
    pub mode: Mode,
}

/// What the front end does while a question runs. Every method has a
/// default, so a caller implements only what it needs.
pub trait Hooks {
    /// Called before each request goes, with the request exactly as it
    /// will be sent, so the user can see it and redact phrases from it
    /// (D-035). Answering `false` stops the question (`ai-chat.stopped`).
    fn review(&mut self, request: &mut PreparedRequest) -> bool {
        let _ = request;
        true
    }

    /// Called with every new line of the tool-call log as it happens.
    fn logged(&mut self, entry: &LogEntry) {
        let _ = entry;
    }

    /// Whether the user stopped the question.
    fn cancelled(&self) -> bool {
        false
    }
}

/// Hooks that let every request go and never stop.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHooks;

impl Hooks for NoHooks {}

/// A stop button for a question that runs on another thread: clones share
/// it.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A button not yet pressed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Presses it.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether it was pressed.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Hooks for Cancel {
    fn cancelled(&self) -> bool {
        self.is_cancelled()
    }
}

/// One conversation with the model about one book.
#[derive(Debug)]
pub struct ChatSession {
    build: BuildInfo,
    mode: Mode,
    /// Whether the provider ever took the functions, after which a refusal
    /// is an error, not a model that can't call them.
    functions_taken: bool,
    /// The conversation without KassenSturz's instructions, which are
    /// built for each request.
    history: Vec<Message>,
    /// Every figure the functions, the user and the build produced so far.
    figures: Figures,
    log: Vec<LogEntry>,
}

impl ChatSession {
    /// A new, empty conversation in the build `build`.
    pub fn new(build: BuildInfo) -> Self {
        let mut figures = Figures::new();
        figures.add_text(&build.version);
        figures.add_text(&build.commit);
        Self {
            build,
            mode: Mode::Functions,
            functions_taken: false,
            history: Vec::new(),
            figures,
            log: Vec::new(),
        }
    }

    /// How the model gets its figures now.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The tool-call log so far.
    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    /// The conversation so far, without KassenSturz's instructions.
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Asks `question` about `book` through `provider`, with `key` when the
    /// provider needs one; `today` is the day of this computer's clock. The
    /// model calls the read functions until it answers, at most
    /// [`MAX_ROUNDS`] requests. When the question ends without an answer
    /// the conversation is as it was before, and the log says why.
    pub fn ask(
        &mut self,
        book: &Book,
        provider: &dyn Provider,
        key: Option<&Secret>,
        question: &str,
        today: Date,
        hooks: &mut dyn Hooks,
    ) -> Result<Answer, ChatError> {
        let start = self.history.len();
        let result = self.run(book, provider, key, question, today, hooks);
        if let Err(error) = &result {
            self.history.truncate(start);
            self.note(
                LogEntry::Failed {
                    code: error.report().code,
                },
                hooks,
            );
        }
        result
    }

    fn run(
        &mut self,
        book: &Book,
        provider: &dyn Provider,
        key: Option<&Secret>,
        question: &str,
        today: Date,
        hooks: &mut dyn Hooks,
    ) -> Result<Answer, ChatError> {
        if hooks.cancelled() {
            return Err(ChatError::Cancelled);
        }
        self.note(
            LogEntry::Asked {
                question: question.to_owned(),
            },
            hooks,
        );
        self.figures.add_text(question);
        self.history.push(Message::user(question));
        let mut overview = None;
        if self.mode == Mode::Overview {
            overview = Some(self.overview(book, today)?);
        }
        let mut round = 0;
        while round < MAX_ROUNDS {
            round += 1;
            if hooks.cancelled() {
                return Err(ChatError::Cancelled);
            }
            let (system, functions) = match &overview {
                Some(overview) => (
                    prompt::with_overview(&self.build, today, overview),
                    Vec::new(),
                ),
                None => (prompt::with_functions(&self.build, today), function_specs()),
            };
            let offered = !functions.is_empty();
            let mut messages = Vec::with_capacity(self.history.len() + 1);
            messages.push(Message::system(system));
            messages.extend(self.history.iter().cloned());
            let mut request = provider.prepare(messages, functions)?;
            if !hooks.review(&mut request) {
                return Err(ChatError::Stopped);
            }
            self.note(
                LogEntry::Sent {
                    round,
                    body: request.preview().to_owned(),
                },
                hooks,
            );
            let reply = match provider.send(&request, key) {
                Ok(reply) => reply,
                // A model that can't call functions: the provider refuses
                // the request that offers them (D-190).
                Err(AiError::ProviderFailed { status: 400 })
                    if offered && !self.functions_taken =>
                {
                    self.mode = Mode::Overview;
                    self.note(LogEntry::Overview, hooks);
                    overview = Some(self.overview(book, today)?);
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            // Whatever came back while the user stopped is thrown away.
            if hooks.cancelled() {
                return Err(ChatError::Cancelled);
            }
            if offered {
                self.functions_taken = true;
            }
            if reply.calls.is_empty() && reply.ignored_calls.is_empty() {
                let unverified = check_numbers(&reply.text, &self.figures);
                self.note(
                    LogEntry::Answered {
                        unverified: unverified.len(),
                    },
                    hooks,
                );
                self.history
                    .push(Message::assistant(reply.text.clone(), Vec::new()));
                return Ok(Answer {
                    text: reply.text,
                    unverified,
                    finish: reply.finish,
                    mode: self.mode,
                });
            }
            self.history
                .push(Message::assistant(reply.text, reply.calls.clone()));
            for call in &reply.calls {
                let message = self.run_call(book, call, today, hooks);
                self.history.push(message);
            }
            if !reply.ignored_calls.is_empty() {
                self.note(
                    LogEntry::NotOffered {
                        calls: reply.ignored_calls.len(),
                    },
                    hooks,
                );
                self.history.push(Message::user(NOT_OFFERED));
            }
        }
        Err(ChatError::TooManyRounds)
    }

    /// Runs one call of the model and answers with the function's result,
    /// or its error code, for the model.
    fn run_call(
        &mut self,
        book: &Book,
        call: &ToolCall,
        today: Date,
        hooks: &mut dyn Hooks,
    ) -> Message {
        let outcome = serde_json::from_str::<Value>(&call.arguments)
            .map_err(|_| ToolError::BadArgument {
                argument: "arguments",
            })
            .and_then(|arguments| kassensturz_ai_tools::call(book, &call.name, &arguments, today));
        let (result, error) = match outcome {
            Ok(output) => {
                self.figures.extend(&output.figures);
                (prompt::data_json(&output.value), None)
            }
            Err(error) => {
                let report = error.report();
                (prompt::data_json(&error_json(&report)), Some(report.code))
            }
        };
        self.note(
            LogEntry::Called {
                function: call.name.clone(),
                arguments: cut(&call.arguments, MAX_ARGUMENT_CHARS),
                result: result.clone(),
                error,
            },
            hooks,
        );
        Message::tool_result(call.id.clone(), result)
    }

    /// The overview for a model that can't call functions, with its figures
    /// added to those the answer may name.
    fn overview(&mut self, book: &Book, today: Date) -> Result<Value, ChatError> {
        let output = kassensturz_ai_tools::overview(book, today).map_err(ChatError::Book)?;
        self.figures.extend(&output.figures);
        Ok(output.value)
    }

    fn note(&mut self, entry: LogEntry, hooks: &mut dyn Hooks) {
        hooks.logged(&entry);
        self.log.push(entry);
    }
}

/// The first request of a question, as the provider would get it for
/// `question` asked without any earlier conversation, but with nothing
/// sent: the screens show it as an example of what a request holds
/// (M5.7). Its parts are the ones [`ChatSession::ask`] builds, and a test
/// holds the two together.
pub fn sample_request(
    provider: &dyn Provider,
    build: &BuildInfo,
    today: Date,
    question: &str,
) -> Result<PreparedRequest, AiError> {
    provider.prepare(
        vec![
            Message::system(prompt::with_functions(build, today)),
            Message::user(question),
        ],
        function_specs(),
    )
}

/// The read functions, as the provider gets them.
fn function_specs() -> Vec<FunctionSpec> {
    FUNCTIONS
        .iter()
        .map(|function| FunctionSpec {
            name: function.name.to_owned(),
            description: function.description.to_owned(),
            parameters: serde_json::from_str(function.parameters)
                .unwrap_or_else(|_| json!({"type": "object"})),
        })
        .collect()
}

/// A function's error for the model: its code and the fixed names it
/// carries, never anything the model sent (D-034, X-8).
fn error_json(report: &ErrorReport) -> Value {
    let mut out = Map::new();
    out.insert("error".into(), json!(report.code));
    for (name, value) in &report.params {
        match value {
            ErrorParam::Keyword(keyword) => {
                out.insert((*name).into(), json!(keyword));
            }
            ErrorParam::Number(number) => {
                out.insert((*name).into(), json!(number));
            }
            _ => {}
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_is_offered_with_its_parameters() {
        let specs = function_specs();
        assert_eq!(specs.len(), FUNCTIONS.len());
        for (spec, function) in specs.iter().zip(FUNCTIONS) {
            assert_eq!(spec.name, function.name);
            assert_eq!(
                spec.parameters,
                serde_json::from_str::<Value>(function.parameters).unwrap(),
                "{}'s parameters are JSON",
                function.name
            );
        }
    }

    #[test]
    fn an_error_carries_only_its_code_and_fixed_names() {
        let report = ToolError::BadArgument { argument: "from" }.report();
        assert_eq!(
            error_json(&report),
            json!({"error": "ai-tools.bad-argument", "argument": "from"})
        );
    }

    #[test]
    fn a_cancel_is_shared_by_its_clones() {
        let cancel = Cancel::new();
        let other = cancel.clone();
        assert!(!other.cancelled());
        cancel.cancel();
        assert!(other.cancelled());
    }
}
