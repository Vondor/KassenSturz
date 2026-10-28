// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One conversation with the model: the loop of the read functions, the
//! fallback to the overview, the number check and the tool-call log (D-190).

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use kassensturz_ai::{
    AiError, Finish, FunctionSpec, Message, PreparedRequest, Provider, Role, ToolCall,
};
use kassensturz_ai_tools::{
    ChangeSet, DRAFT_ISSUE, FUNCTIONS, Figures, Function, IssueContext, IssueDraft,
    PROPOSE_CHANGES, ToolError, Unverified, check_numbers, draft_issue, propose, propose_spec,
    unique_codes,
};
#[cfg(feature = "debug-log")]
use kassensturz_ai_tools::{MAX_LOG_ENTRIES, READ_DEBUG_LOG, read_debug_log};
use kassensturz_core::ai_switch::{self, Permissions};
#[cfg(feature = "debug-log")]
use kassensturz_core::debug_log::DebugLog;
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

/// The smallest request, in bytes, whose token count is compared with what
/// the provider reports.
const CUT_MIN_BYTES: usize = 12_000;

/// More bytes than this per token the provider counted means it did not read
/// the whole request. The request's JSON, with IDs and amounts, comes to
/// about 5 bytes per token; the cut requests of the local-model test (M5-B23)
/// came to 15 to 18.
const CUT_BYTES_PER_TOKEN: u64 = 9;

/// Whether a request of `bytes` that the provider counted as `tokens` was
/// probably cut to the model's context length, so the model did not see all
/// of it.
pub fn looks_cut(bytes: usize, tokens: Option<u64>) -> bool {
    let Some(tokens) = tokens else {
        return false;
    };
    bytes >= CUT_MIN_BYTES && bytes as u64 > tokens.saturating_mul(CUT_BYTES_PER_TOKEN)
}

/// Said to the model for the last request of a question: when it asks for
/// the same call again, or the requests run out. A small model that can't
/// stop calling is made to answer from what it has, with no function offered
/// (M5-B24, D-240).
pub const ANSWER_NOW: &str = "KassenSturz: you already have the results you need above. \
    Answer the user's question now, in plain words, from those results. No function \
    can be called any more.";

/// Said to the model once when it answered with nothing at all (no text,
/// no call), so the user doesn't have to ask again (#196, D-248).
pub const EMPTY_ANSWER: &str = "KassenSturz: your last reply was empty. Answer the user's \
    question now: call a function if you need data, otherwise answer in plain words.";

/// Whether a reply's text is a function call the provider's template did
/// not turn into a call, as some local models write it, e.g.
/// `<|tool_call>call:list_categories{}<tool_call|>` (#196, D-248).
fn is_raw_tool_call(text: &str) -> bool {
    let text = text.to_lowercase();
    [
        "<|tool_call",
        "<tool_call",
        "[tool_calls]",
        "<function=",
        "<|python_tag|>",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

/// What a function's result becomes once its question is over. A small
/// model's context (LM Studio starts with 4096 tokens) holds the
/// instructions and the functions and little more, and the whole results of
/// earlier questions made the provider cut off the newest message (M5-B23,
/// D-237). The call stays, so the model still sees that its answer came from a
/// function (#186, D-242). The note says to call for each new question and
/// does not invite it to repeat the same call (M5-B24).
pub const SHORTENED_RESULT: &str = r#"{"note":"KassenSturz left out this result of an earlier question to save space. Its figures are in the answer that follows. For the question you are answering now, call the functions you need before you answer."}"#;

/// What the model is told when it asked for a function that isn't offered.
/// It goes as a message of its own because a function's answer must name a
/// call the provider knows, and such a call was never handed on (threat
/// T-31, D-190).
pub const NOT_OFFERED: &str = "KassenSturz: a function you asked for is not offered, so it \
                               was not run (error ai-tools.unknown-function). Call only the \
                               functions offered.";

/// What the user did with the changes the model prepared, which the model
/// hears at its next question (D-229).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The user confirmed them and they were made.
    Confirmed,
    /// The user threw them away; nothing was changed.
    Declined,
    /// The user confirmed them but they could not be made; nothing was
    /// changed.
    Failed,
    /// The user took the confirmed changes back with Undo.
    Undone,
}

impl Outcome {
    /// What the model is told. Fixed text from KassenSturz, nothing from
    /// the book.
    pub fn text(self) -> &'static str {
        match self {
            Self::Confirmed => {
                "KassenSturz: the user reviewed the changes you prepared in KassenSturz's chat \
                 sidebar and confirmed them. They were made in the user's book. Nothing \
                 waits for approval now."
            }
            Self::Declined => {
                "KassenSturz: the user reviewed the changes you prepared and refused them. \
                 Nothing was changed in the user's book, and nothing waits for approval now."
            }
            Self::Failed => {
                "KassenSturz: the user confirmed the changes you prepared, but they could not \
                 be made, so nothing was changed in the user's book. Nothing waits for \
                 approval now."
            }
            Self::Undone => {
                "KassenSturz: the user took the changes you prepared back with Undo. They are \
                 no longer in the user's book."
            }
        }
    }
}

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
    /// The provider counted far fewer tokens than were sent, so it probably
    /// cut the request to the model's context length and the model did not
    /// see all of it (M5-B23, D-238).
    pub context_cut: bool,
    /// The changes the model prepared for this question, in the order it
    /// called `propose_changes`, for the user to review (D-171, D-199).
    /// Nothing in them is applied: only the review screen's confirm does.
    pub changes: Vec<ChangeSet>,
    /// The issue draft the model prepared for this question, the last one
    /// if it made several, for the user to read (D-173, D-228). Nothing is
    /// sent: only the user opens the prefilled page.
    pub issue: Option<IssueDraft>,
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
    /// What the user did with prepared changes since the last question, to
    /// tell the model with the next one.
    outcomes: Vec<Outcome>,
    /// The change sets the model prepared for the question under way.
    proposed: Vec<ChangeSet>,
    /// The issue draft the model prepared for the question under way.
    issue: Option<IssueDraft>,
    /// The debug log the conversation writes to and the model may read
    /// while AI and the log are on (D-173).
    #[cfg(feature = "debug-log")]
    debug_log: DebugLog,
    /// The screen the user asked from, for an issue draft.
    screen: Option<&'static str>,
    /// Which changes the model may prepare, fixed for tests; `None` reads
    /// the user's settings for each request (D-239).
    permissions: Option<Permissions>,
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
            outcomes: Vec::new(),
            proposed: Vec::new(),
            issue: None,
            #[cfg(feature = "debug-log")]
            debug_log: DebugLog::none(),
            screen: None,
            permissions: None,
        }
    }

    /// The conversation with the permissions fixed to `permissions`,
    /// instead of read from the user's settings for each request.
    #[must_use]
    pub fn with_permissions(mut self, permissions: Permissions) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// Which changes the model may prepare now (D-239). Asked again for
    /// each request and each call: the user may change it while a
    /// question runs.
    fn permissions(&self) -> Permissions {
        self.permissions.unwrap_or_else(ai_switch::permissions)
    }

    /// The conversation, writing to `log` while the developer mode is on
    /// and letting the model read it while AI is on too (D-173, D-227).
    /// Without one, nothing is written and the model isn't offered the log.
    #[cfg(feature = "debug-log")]
    pub fn with_debug_log(mut self, log: DebugLog) -> Self {
        self.debug_log = log;
        self
    }

    /// The screen the user asks from, a name from the debug log's fixed
    /// list, for an issue draft.
    pub fn set_screen(&mut self, screen: Option<&'static str>) {
        self.screen = screen;
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

    /// Remembers what the user did with changes the model prepared, so the
    /// model hears it as a message from KassenSturz before the next
    /// question and doesn't go on saying they wait (D-229).
    pub fn note_outcome(&mut self, outcome: Outcome) {
        self.outcomes.push(outcome);
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
        // Kept even when the question fails: it happened.
        for outcome in std::mem::take(&mut self.outcomes) {
            self.history.push(Message::user(outcome.text()));
        }
        // The calls of earlier questions stay, so the model keeps seeing that
        // figures come from functions (#186: without them small models
        // stopped calling and invented figures from about the fourth
        // question). Their results are shortened because they fill a small
        // model's context (D-237).
        for message in &mut self.history {
            if message.role == Role::Tool {
                message.content = SHORTENED_RESULT.to_owned();
            }
        }
        let start = self.history.len();
        #[cfg(feature = "debug-log")]
        self.debug_log.start_turn();
        let result = self.run(book, provider, key, question, today, hooks);
        if let Err(error) = &result {
            self.history.truncate(start);
            #[cfg(feature = "debug-log")]
            self.debug_log.transcript(
                "failed",
                &[
                    ("code", error.report().code),
                    ("detail", &error.to_string()),
                ],
            );
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
        self.proposed.clear();
        self.issue = None;
        let mut overview = None;
        let mut context_cut = false;
        if self.mode == Mode::Overview {
            overview = Some(self.overview(book, today)?);
        }
        let mut round = 0;
        // The calls made for this question, and whether the next request
        // is the one that has to answer.
        let mut made: HashSet<(String, String)> = HashSet::new();
        let mut closing = false;
        let mut asked_again = false;
        while round < MAX_ROUNDS {
            round += 1;
            if round == MAX_ROUNDS && !closing {
                closing = true;
                self.history.push(Message::user(ANSWER_NOW));
            }
            if hooks.cancelled() {
                return Err(ChatError::Cancelled);
            }
            // Asked again for each request: the user may turn the log on
            // or off while a question runs.
            let log_readable = self.log_readable();
            let permissions = self.permissions();
            let (system, functions) = match &overview {
                Some(overview) => (
                    prompt::with_overview(&self.build, today, overview),
                    Vec::new(),
                ),
                None => (
                    prompt::with_functions(&self.build, today, log_readable, permissions),
                    if closing {
                        Vec::new()
                    } else {
                        function_specs(log_readable, permissions)
                    },
                ),
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
            let sent = provider.send_until(&request, key, &|| hooks.cancelled());
            // Whatever came back while the user stopped is thrown away,
            // an error too: Stop and Cancel end the wait (D-246).
            if hooks.cancelled() {
                return Err(ChatError::Cancelled);
            }
            self.transcribe_reply(provider, sent.as_ref().err());
            let reply = match sent {
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
            context_cut |= looks_cut(request.preview().len(), reply.prompt_tokens);
            if reply.calls.is_empty() && reply.ignored_calls.is_empty() {
                // Nothing came back because the context was full: the
                // request and the answer didn't fit (M5-B33).
                if reply.text.trim().is_empty() && reply.finish == Finish::Length {
                    return Err(AiError::ContextTooSmall.into());
                }
                // An empty reply that ended normally (not a refusal, a filter or a
                // cut) is asked for once more before the user has
                // to repeat the question.
                if reply.text.trim().is_empty()
                    && reply.finish == Finish::Done
                    && !asked_again
                    && !closing
                {
                    asked_again = true;
                    self.history.push(Message::user(EMPTY_ANSWER));
                    continue;
                }
                if is_raw_tool_call(&reply.text) {
                    return Err(ChatError::ToolCallAsText);
                }
                let unverified = check_numbers(&reply.text, &self.figures);
                #[cfg(feature = "debug-log")]
                self.debug_log.transcript(
                    "answer",
                    &[
                        ("text", &reply.text),
                        ("unverified", &unverified.len().to_string()),
                    ],
                );
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
                    context_cut,
                    changes: std::mem::take(&mut self.proposed),
                    issue: self.issue.take(),
                });
            }
            if closing {
                // Asked to answer and still calling: nothing more to try.
                return Err(ChatError::TooManyRounds);
            }
            self.history
                .push(Message::assistant(reply.text, reply.calls.clone()));
            let mut repeated = false;
            for call in &reply.calls {
                repeated |= !made.insert((call.name.clone(), call.arguments.clone()));
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
            // The same call again gets the same result: the model is going
            // round in a circle, so its next request has to be the answer.
            if repeated && !closing {
                closing = true;
                self.history.push(Message::user(ANSWER_NOW));
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
            .and_then(|arguments| {
                if call.name == PROPOSE_CHANGES.name {
                    // Prepared for the user, never applied here (T-27).
                    propose(book, &arguments, self.permissions()).map(|proposal| {
                        self.proposed.push(proposal.changes);
                        proposal.output
                    })
                } else if call.name == DRAFT_ISSUE.name {
                    // Shown to the user, never sent from here (T-33).
                    draft_issue(book, &arguments, &self.issue_context()).map(|(draft, output)| {
                        self.issue = Some(draft);
                        output
                    })
                } else {
                    self.read(book, &call.name, &arguments, today)
                }
            });
        let (result, error) = match outcome {
            Ok(output) => {
                self.figures.extend(&output.figures);
                (prompt::data_json(&output.value), None)
            }
            Err(error) => {
                let report = error.report();
                let prepares = call.name == PROPOSE_CHANGES.name;
                (
                    prompt::data_json(&error_json(&report, prepares)),
                    Some(report.code),
                )
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

    /// Whether the model may read the debug log now: only while AI and the
    /// log are both on (D-173), never in a build without the log.
    fn log_readable(&self) -> bool {
        #[cfg(feature = "debug-log")]
        return self.debug_log.readable_by_ai();
        #[cfg(not(feature = "debug-log"))]
        false
    }

    /// Runs the read function `name`: one of the book's, or the one that
    /// reads the debug log.
    fn read(
        &self,
        book: &Book,
        name: &str,
        arguments: &Value,
        today: Date,
    ) -> Result<kassensturz_ai_tools::Output, ToolError> {
        #[cfg(feature = "debug-log")]
        if name == READ_DEBUG_LOG.name {
            return read_debug_log(&self.debug_log, arguments);
        }
        kassensturz_ai_tools::call(book, name, arguments, today)
    }

    /// What KassenSturz adds to an issue draft: the build, the screen, and
    /// the error codes of this conversation and, while AI may read it, of
    /// the debug log.
    fn issue_context(&self) -> IssueContext {
        #[cfg(feature = "debug-log")]
        let (log_read, logged) = {
            let readable = self.debug_log.readable_by_ai();
            let codes: Vec<String> = if readable {
                self.debug_log
                    .read(MAX_LOG_ENTRIES)
                    .into_iter()
                    .filter_map(|entry| entry.code)
                    .collect()
            } else {
                Vec::new()
            };
            (readable, codes)
        };
        #[cfg(not(feature = "debug-log"))]
        let (log_read, logged) = (false, Vec::<String>::new());
        let conversation = self.log.iter().filter_map(|entry| match entry {
            LogEntry::Called { error, .. } => *error,
            LogEntry::Failed { code } => Some(*code),
            _ => None,
        });
        IssueContext {
            version: self.build.version.clone(),
            commit: self.build.commit.clone(),
            screen: self.screen,
            codes: unique_codes(logged.iter().map(String::as_str).chain(conversation)),
            log_read,
        }
    }

    /// The provider's reply as it came, and why it couldn't be read when it
    /// couldn't, for the developer mode's transcript (D-235).
    fn transcribe_reply(&self, provider: &dyn Provider, error: Option<&AiError>) {
        #[cfg(feature = "debug-log")]
        {
            let raw = provider.last_reply_text().unwrap_or_default();
            let code = error.map_or("", |error| error.report().code);
            let detail = error.map(ToString::to_string).unwrap_or_default();
            self.debug_log.transcript(
                "reply",
                &[("raw", &raw), ("error", code), ("detail", &detail)],
            );
        }
        #[cfg(not(feature = "debug-log"))]
        let _ = (provider, error);
    }

    fn note(&mut self, entry: LogEntry, hooks: &mut dyn Hooks) {
        #[cfg(feature = "debug-log")]
        {
            let (event, detail, code) = entry.debug_event();
            self.debug_log.record(event, detail, code);
            entry.transcribe(&self.debug_log);
        }
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
    // As a real question would get it, with the user's permissions now.
    let permissions = ai_switch::permissions();
    provider.prepare(
        vec![
            Message::system(prompt::with_functions(build, today, false, permissions)),
            Message::user(question),
        ],
        function_specs(false, permissions),
    )
}

/// The read functions, the one that prepares changes (only the lists the
/// user allows, and not at all when they allow none, D-239) and the one
/// that drafts an issue, as the provider gets them, and the one that reads
/// the debug log while the model may read it.
fn function_specs(log_readable: bool, permissions: Permissions) -> Vec<FunctionSpec> {
    let spec = |function: &Function| FunctionSpec {
        name: function.name.to_owned(),
        description: function.description.to_owned(),
        parameters: serde_json::from_str(function.parameters)
            .unwrap_or_else(|_| json!({"type": "object"})),
    };
    let mut specs: Vec<FunctionSpec> = FUNCTIONS.iter().map(spec).collect();
    specs.extend(propose_spec(permissions).map(|propose| FunctionSpec {
        name: propose.name.to_owned(),
        description: propose.description,
        parameters:
            serde_json::from_str(&propose.parameters).unwrap_or_else(|_| json!({"type": "object"})),
    }));
    specs.push(spec(&DRAFT_ISSUE));
    #[cfg(feature = "debug-log")]
    if log_readable {
        specs.push(spec(&READ_DEBUG_LOG));
    }
    #[cfg(not(feature = "debug-log"))]
    let _ = log_readable;
    specs
}

/// What the model is told when an ID it sent is not one of the book's: the
/// model once told the user to call the function themselves instead.
const ID_HINT: &str = "An ID must be copied from a function's result. Call list_categories \
(categories), list_accounts (accounts), search_transactions (transactions) or list_rules \
(Rules) yourself now, then call this function again with the ID. Don't ask the user to do it.";

/// What the model is told when it wrote the wrong one of two arguments for
/// a category (#195, #212): `new_category` and `new_parent` are only keys of
/// categories made in the same call, and a category that exists is named by
/// `category_id` or `parent_id`, with an ID and never a name.
const NEW_CATEGORY_HINT: &str = "new_category is only the key of a category made in \
new_categories of this same call. For a category that exists, use category_id with an ID \
copied from list_categories.";
const NEW_PARENT_HINT: &str = "new_parent is only the key of a category made in \
new_categories of this same call. For a category that exists, use parent_id with an ID \
copied from list_categories.";
const CATEGORY_ID_HINT: &str = "category_id and parent_id take the ID copied from \
list_categories, not the name of the category.";

/// What the model is told when a call to `propose_changes` failed.
const NOT_PREPARED_HINT: &str = "Nothing was prepared. Do not tell the user that a change is \
prepared. Fix the call and call propose_changes again, or say plainly that it did not work.";

/// A function's error for the model: its code and the fixed names it
/// carries, never anything the model sent (D-034, X-8), and fixed hints
/// when an ID was wrong, a key was used for an ID, or the call that failed
/// was to prepare changes (`prepares`).
fn error_json(report: &ErrorReport, prepares: bool) -> Value {
    let mut out = Map::new();
    out.insert("error".into(), json!(report.code));
    let mut wrong_id = report.code.starts_with("ai-tools.unknown-");
    let mut hints = Vec::new();
    for (name, value) in &report.params {
        match value {
            ErrorParam::Keyword(keyword) => {
                wrong_id |= *name == "argument" && keyword.ends_with("id");
                if *name == "argument" {
                    match *keyword {
                        "new_category" => hints.push(NEW_CATEGORY_HINT),
                        "new_parent" => hints.push(NEW_PARENT_HINT),
                        "category_id" | "parent_id" => hints.push(CATEGORY_ID_HINT),
                        _ => {}
                    }
                }
                out.insert((*name).into(), json!(keyword));
            }
            ErrorParam::Number(number) => {
                out.insert((*name).into(), json!(number));
            }
            _ => {}
        }
    }
    if wrong_id {
        hints.push(ID_HINT);
    }
    if prepares {
        hints.push(NOT_PREPARED_HINT);
    }
    if !hints.is_empty() {
        out.insert("hint".into(), json!(hints.join(" ")));
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_cut_when_the_provider_counts_far_fewer_tokens() {
        // Numbers of the local-model test (M5-B23): cut requests ...
        assert!(looks_cut(63_359, Some(3_865)));
        assert!(looks_cut(97_500, Some(5_356)));
        assert!(looks_cut(58_709, Some(3_872)));
        // ... and uncut ones, with their real token counts.
        assert!(!looks_cut(17_992, Some(3_458)));
        assert!(!looks_cut(20_193, Some(3_458)));
        // A short request or an unknown count is never called cut.
        assert!(!looks_cut(5_000, Some(10)));
        assert!(!looks_cut(63_359, None));
        assert!(!looks_cut(usize::MAX, Some(u64::MAX)));
    }

    #[test]
    fn every_function_is_offered_with_its_parameters() {
        let specs = function_specs(false, Permissions::all());
        assert_eq!(specs.len(), FUNCTIONS.len() + 2);
        assert_eq!(specs[FUNCTIONS.len()].name, PROPOSE_CHANGES.name);
        assert_eq!(specs[FUNCTIONS.len() + 1].name, DRAFT_ISSUE.name);
        for (spec, function) in specs
            .iter()
            .zip(FUNCTIONS.iter().chain([&PROPOSE_CHANGES, &DRAFT_ISSUE]))
        {
            assert_eq!(spec.name, function.name);
            assert_eq!(
                spec.parameters,
                serde_json::from_str::<Value>(function.parameters).unwrap(),
                "{}'s parameters are JSON",
                function.name
            );
        }
        // The debug log is offered only while the model may read it, and
        // never in a build without it.
        let with_log = function_specs(true, Permissions::all());
        #[cfg(feature = "debug-log")]
        {
            assert_eq!(with_log.len(), specs.len() + 1);
            assert_eq!(with_log[specs.len()].name, READ_DEBUG_LOG.name);
            assert_eq!(
                with_log[specs.len()].parameters,
                serde_json::from_str::<Value>(READ_DEBUG_LOG.parameters).unwrap()
            );
        }
        #[cfg(not(feature = "debug-log"))]
        assert_eq!(with_log.len(), specs.len());
        assert_eq!(with_log[..specs.len()], specs[..]);
    }
    #[test]
    fn an_error_carries_only_its_code_and_fixed_names() {
        let report = ToolError::BadArgument { argument: "from" }.report();
        assert_eq!(
            error_json(&report, false),
            json!({"error": "ai-tools.bad-argument", "argument": "from"})
        );
    }

    #[test]
    fn a_wrong_id_comes_with_a_hint_to_look_it_up() {
        let wrong = ToolError::BadArgument {
            argument: "parent_id",
        }
        .report();
        let hint = error_json(&wrong, false)["hint"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(hint, format!("{CATEGORY_ID_HINT} {ID_HINT}"));
        let other = ToolError::BadArgument { argument: "from" }.report();
        assert!(error_json(&other, false).get("hint").is_none());
    }

    #[test]
    fn a_key_used_for_an_id_and_a_failed_proposal_get_their_hints() {
        let key = ToolError::BadArgument {
            argument: "new_category",
        }
        .report();
        assert_eq!(error_json(&key, false)["hint"], json!(NEW_CATEGORY_HINT));
        let parent = ToolError::BadArgument {
            argument: "new_parent",
        }
        .report();
        assert_eq!(error_json(&parent, false)["hint"], json!(NEW_PARENT_HINT));
        let both = error_json(&key, true)["hint"].as_str().unwrap().to_owned();
        assert!(both.contains(NEW_CATEGORY_HINT) && both.contains(NOT_PREPARED_HINT));
        let other = ToolError::BadArgument { argument: "from" }.report();
        assert_eq!(error_json(&other, true)["hint"], json!(NOT_PREPARED_HINT));
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
