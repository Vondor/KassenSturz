// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One conversation with the model: the loop of the read functions, the
//! fallback to the overview, the number check and the tool-call log (D-190).

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use kassensturz_ai::{
    AiError, Finish, FunctionSpec, MAX_ANSWER_TOKENS, Message, PreparedRequest, Provider, Role,
    ToolCall,
};
use kassensturz_ai_tools::refusal::{REFUSAL_KEYWORDS, RefusalReasons};
use kassensturz_ai_tools::{
    ChangeSet, DRAFT_ISSUE, Figures, Function, IssueContext, IssueDraft, MAX_VIEWS_PER_ANSWER,
    OPEN_PAGE, PROPOSE_CHANGES, PROPOSE_VIEW_NAME, PageTarget, ProposedChange, ToolError,
    Unverified, check_numbers, description_titles, draft_issue, open_page_allowed, propose_on,
    propose_spec, propose_view_spec, propose_view_text, readable_functions, unique_codes,
    view_choices,
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

/// Whether a server's refusal names tools or functions, as Ollama's "does
/// not support tools" does: then the model can't call them and there is no
/// point in asking again (D-595).
fn names_functions(refusal: Option<&str>) -> bool {
    refusal.is_some_and(|text| {
        let text = text.to_lowercase();
        text.contains("tool") || text.contains("function")
    })
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

/// How often the model may correct a refused call of `propose_view` in one
/// question: the call and two corrections (D-355, D-402). The next refusal
/// ends the attempt, and the chat offers the Statistic Builder instead.
pub const MAX_VIEW_CORRECTIONS: usize = 2;

/// Said to the model once when its corrections of `propose_view` ran out, as
/// the last request of the question. The chat card tells the user itself
/// that no chart could be prepared (D-402).
pub const CHART_NOT_PREPARED: &str = "KassenSturz: the chart could not be prepared, and no \
    more corrections are allowed. Do not call propose_view again. Tell the user in one or two \
    plain sentences that you could not prepare the chart and that they can make it in the \
    Statistic Builder. Do not describe what the chart would have shown.";

/// What the user did with the changes the model prepared, which the model
/// hears at its next question (D-229).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The user confirmed them and they were made.
    Confirmed,
    /// The user changed or left out some of them before confirming, and
    /// the rest were made as the user left them (D-298). What the user
    /// changed is not said.
    ConfirmedChanged,
    /// The user threw them away, with the reasons they picked from a fixed
    /// list, if any (D-678); nothing was changed.
    Declined(RefusalReasons),
    /// The user confirmed them but they could not be made; nothing was
    /// changed.
    Failed,
    /// The user took the confirmed changes back with Undo.
    Undone,
}

impl Outcome {
    /// What the model is told. Fixed text from KassenSturz, nothing from
    /// the book and nothing the user typed: a refusal's reasons are keywords
    /// of a fixed list.
    pub fn text(self) -> String {
        if let Self::Declined(reasons) = self
            && !reasons.is_empty()
        {
            return format!(
                "KassenSturz: the user reviewed the changes you prepared and refused them. \
                 Nothing was changed in the user's book, and nothing waits for approval now. \
                 The reasons the user picked from a fixed list: {}. The list's keywords mean: \
                 {REFUSAL_KEYWORDS}. Ask the user what to do differently before you prepare \
                 changes again.",
                reasons.keywords().join(", ")
            );
        }
        let text = match self {
            Self::Confirmed => {
                "KassenSturz: the user reviewed the changes you prepared in KassenSturz's chat \
                 sidebar and confirmed them. They were made in the user's book. Nothing \
                 waits for approval now."
            }
            Self::ConfirmedChanged => {
                "KassenSturz: the user reviewed the changes you prepared in KassenSturz's chat \
                 sidebar, changed or left out some of them, and confirmed the rest. They were \
                 made in the user's book as the user changed them, which may differ from what \
                 you prepared. Before you speak about them, read them with the functions; do \
                 not assume your version. Nothing waits for approval now."
            }
            Self::Declined(_) => {
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
        };
        text.to_owned()
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

/// A chart the model could not prepare, even after the corrections
/// [`MAX_VIEW_CORRECTIONS`] allows (D-402). The chat says so plainly and
/// offers the Statistic Builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartFailed {
    /// The code of the last refusal.
    pub code: &'static str,
    /// The dataset the last call named, when that was one the AI may name:
    /// what it understood, which the builder starts from. Always one of the
    /// catalog's own ids, never the model's text.
    pub dataset: Option<&'static str>,
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
    /// For each set of `changes`, the numbers in the titles of its new
    /// charts that no function produced, marked like those of an answer
    /// (D-188, D-402).
    pub title_numbers: Vec<Vec<Unverified>>,
    /// Set when the model's attempts at a chart ran out (D-402).
    pub chart_failed: Option<ChartFailed>,
    /// Set when the model's last attempt to prepare a change or a chart
    /// for this question was refused and nothing was prepared, so the chat
    /// says in its own words that no change waits, whatever the model
    /// claims (Q-414, D-595).
    pub no_change: bool,
    /// The issue draft the model prepared for this question, the last one
    /// if it made several, for the user to read (D-173, D-228). Nothing is
    /// sent: only the user opens the prefilled page.
    pub issue: Option<IssueDraft>,
    /// The page the model asked to open for this question, the last one if
    /// it asked for several (D-265). Opening it changes nothing in the
    /// book; the front end opens it when the answer is shown and says so.
    pub page: Option<PageTarget>,
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
    /// Whether the connection test found that this model does not call
    /// functions (D-324). Set by the app before each question.
    test_found_no_functions: bool,
    /// Whether the overview mode is on because of that test and not
    /// because the provider refused the functions.
    overview_from_test: bool,
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
    /// The page the model asked to open for the question under way.
    page: Option<PageTarget>,
    /// How many calls of `propose_view` were refused in the question under
    /// way (D-402).
    view_refusals: usize,
    /// Set when the refusals went past [`MAX_VIEW_CORRECTIONS`].
    chart_failed: Option<ChartFailed>,
    /// Whether the last call of `propose_changes` or `propose_view` in the
    /// question under way was refused.
    last_change_refused: bool,
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
            test_found_no_functions: false,
            overview_from_test: false,
            history: Vec::new(),
            figures,
            log: Vec::new(),
            outcomes: Vec::new(),
            proposed: Vec::new(),
            issue: None,
            page: None,
            view_refusals: 0,
            chart_failed: None,
            last_change_refused: false,
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
    /// Bills are attached only by the user's own AI tools over MCP, which
    /// read the bill themselves: the chat reads no PDF, so it is never
    /// offered `attach_bills` (Q-506, D-739).
    fn permissions(&self) -> Permissions {
        self.permissions
            .unwrap_or_else(ai_switch::permissions)
            .with(ai_switch::Permission::AttachBills, false)
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

    /// Tells the conversation what the last "Test connection" found out
    /// about the model: `true` when it did not call the test function
    /// (D-324). The next question then starts in the overview mode instead
    /// of offering functions the model would not call; `false` takes the
    /// chat back to the functions when it was the test that moved it. A
    /// conversation in which the provider already took the functions is
    /// left alone: what happened here beats what a test found.
    pub fn set_test_found_no_functions(&mut self, found: bool) {
        self.test_found_no_functions = found;
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
        self.follow_the_test(hooks);
        self.figures.add_text(question);
        self.history.push(Message::user(question));
        self.proposed.clear();
        self.issue = None;
        self.page = None;
        self.view_refusals = 0;
        self.chart_failed = None;
        self.last_change_refused = false;
        // The whole question's time limit, when the provider has one
        // (M6-B172, D-595).
        let deadline = provider
            .question_limit()
            .map(|limit| Instant::now() + limit);
        let too_late = || deadline.is_some_and(|deadline| Instant::now() >= deadline);
        let mut retried_failure = false;
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
            if too_late() {
                return Err(ChatError::TooSlow);
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
            let sent = provider.send_until(&request, key, &|| hooks.cancelled() || too_late());
            // Whatever came back while the user stopped is thrown away,
            // an error too: Stop and Cancel end the wait (D-246).
            if hooks.cancelled() {
                return Err(ChatError::Cancelled);
            }
            if sent.is_err() && too_late() {
                return Err(ChatError::TooSlow);
            }
            self.transcribe_reply(provider, sent.as_ref().err());
            let mut reply = match sent {
                Ok(reply) => reply,
                // A server's passing hiccup (LM Studio's "fetch failed")
                // is also a 400: unless the server's words name tools or
                // functions, the same request is tried once more before the
                // chat changes its mode (M6-B173, D-595).
                Err(AiError::ProviderFailed { status: 400 })
                    if offered
                        && !self.functions_taken
                        && !retried_failure
                        && !names_functions(provider.last_reply_text().as_deref()) =>
                {
                    // The log shows it as a second request of the round.
                    retried_failure = true;
                    continue;
                }
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
                    // It used up the answer length KassenSturz asked for,
                    // thinking most likely, before writing an answer
                    // (M6-B172, D-595); otherwise the context was full.
                    if reply
                        .completion_tokens
                        .is_some_and(|tokens| tokens >= MAX_ANSWER_TOKENS)
                    {
                        return Err(ChatError::AnswerTooLong);
                    }
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
                // Still nothing after a normal end, and nothing prepared
                // either: say so instead of showing an empty answer, e.g.
                // when a server put the whole answer in its thinking field
                // (M6-B169, D-593). A refusal or a filter (another finish)
                // reaches the chat panel with its reason, as before.
                if reply.text.trim().is_empty()
                    && reply.finish == Finish::Done
                    && self.proposed.is_empty()
                    && self.issue.is_none()
                    && self.page.is_none()
                    && self.chart_failed.is_none()
                {
                    return Err(ChatError::EmptyAnswer);
                }
                // Marks like ** show as characters in the plain-text chat
                // (M5-B46); the figures are checked in the text the user reads.
                reply.text = crate::plain::plain_answer(&reply.text);
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
                let changes = std::mem::take(&mut self.proposed);
                let no_change =
                    self.last_change_refused && changes.is_empty() && self.chart_failed.is_none();
                let title_numbers = changes.iter().map(|set| self.title_numbers(set)).collect();
                return Ok(Answer {
                    text: reply.text,
                    unverified,
                    finish: reply.finish,
                    mode: self.mode,
                    context_cut,
                    changes,
                    title_numbers,
                    no_change,
                    chart_failed: self.chart_failed.take(),
                    issue: self.issue.take(),
                    page: self.page.take(),
                });
            }
            if closing {
                // Asked to answer and still calling: nothing more to try.
                return Err(ChatError::TooManyRounds);
            }
            self.history
                .push(Message::assistant(reply.text, reply.calls.clone()));
            // A call is repeated when an earlier request of this question
            // made it. The same call twice in one reply runs once and both
            // get its result, so it prepares nothing twice (M6-B170, D-593).
            let mut repeated = false;
            let mut this_reply: Vec<((String, String), String)> = Vec::new();
            for call in &reply.calls {
                let key = (call.name.clone(), call.arguments.clone());
                if let Some((_, content)) = this_reply.iter().find(|(made, _)| *made == key) {
                    self.history
                        .push(Message::tool_result(call.id.clone(), content.clone()));
                    continue;
                }
                repeated |= made.contains(&key);
                let message = self.run_call(book, call, today, hooks);
                this_reply.push((key, message.content.clone()));
                self.history.push(message);
            }
            made.extend(this_reply.into_iter().map(|(key, _)| key));
            if !reply.ignored_calls.is_empty() {
                self.note(
                    LogEntry::NotOffered {
                        calls: reply.ignored_calls.len(),
                    },
                    hooks,
                );
                self.history.push(Message::user(NOT_OFFERED));
            }
            // The corrections of a chart ran out: the next request is the
            // one that says so, with no function offered (D-402).
            if self.chart_failed.is_some() && !closing {
                closing = true;
                self.history.push(Message::user(CHART_NOT_PREPARED));
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
        let views = self
            .proposed
            .iter()
            .flat_map(ChangeSet::changes)
            .filter(|change| {
                matches!(
                    change,
                    ProposedChange::NewView(_) | ProposedChange::EditView(_)
                )
            })
            .count();
        let mut view_refused = false;
        let outcome = if call.name == PROPOSE_VIEW_NAME {
            // The text itself, so a key written twice is seen (D-353).
            // Prepared for the user, never applied here (T-27, T-46).
            if views >= MAX_VIEWS_PER_ANSWER {
                Err(ToolError::TooManyChanges)
            } else {
                propose_view_text(book, &call.arguments, self.permissions(), today)
                    .inspect_err(|_| view_refused = true)
                    .map(|proposal| {
                        self.proposed.push(proposal.changes);
                        proposal.output
                    })
            }
        } else {
            serde_json::from_str::<Value>(&call.arguments)
                .map_err(|_| ToolError::BadArgument {
                    argument: "arguments",
                })
                .and_then(|arguments| {
                    if call.name == PROPOSE_CHANGES.name {
                        // Prepared for the user, never applied here (T-27).
                        propose_on(book, &arguments, self.permissions(), today).map(|proposal| {
                            self.proposed.push(proposal.changes);
                            proposal.output
                        })
                    } else if call.name == DRAFT_ISSUE.name {
                        // Shown to the user, never sent from here (T-33).
                        draft_issue(book, &arguments, &self.issue_context()).map(
                            |(draft, output)| {
                                self.issue = Some(draft);
                                output
                            },
                        )
                    } else if call.name == OPEN_PAGE.name {
                        // Shown to the user when the answer is ready; changes
                        // nothing (T-36).
                        open_page_allowed(book, &arguments, self.permissions()).map(
                            |(page, output)| {
                                self.page = Some(page);
                                output
                            },
                        )
                    } else {
                        self.read(book, &call.name, &arguments, today)
                    }
                })
        };
        if call.name == PROPOSE_CHANGES.name || call.name == PROPOSE_VIEW_NAME {
            self.last_change_refused = outcome.is_err();
        }
        let (result, error) = match outcome {
            Ok(output) => {
                self.figures.extend(&output.figures);
                (prompt::data_json(&output.value), None)
            }
            Err(error) => {
                // The code of the cause, with the list and place of the
                // item it came from, so one answer names both (M6-B191).
                let report = error.model_report();
                if view_refused {
                    self.view_refusals += 1;
                    if self.view_refusals > MAX_VIEW_CORRECTIONS && self.chart_failed.is_none() {
                        self.chart_failed = Some(ChartFailed {
                            code: report.code,
                            dataset: understood_dataset(&call.arguments),
                        });
                    }
                }
                let prepares = call.name == PROPOSE_CHANGES.name;
                let answer = if call.name == PROPOSE_VIEW_NAME {
                    view_error_json(&report)
                } else if call.name == OPEN_PAGE.name {
                    with_hint(error_json(&report, false), NOT_OPENED_HINT)
                } else {
                    error_json(&report, prepares)
                };
                let answer = with_allowed(answer, &error);
                (prompt::data_json(&answer), Some(report.code))
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

    /// The numbers in the titles of the new charts of `set` that no function
    /// produced (D-188, D-402). A title is the model's text: it is shown as
    /// plain text and marked like an answer.
    fn title_numbers(&self, set: &ChangeSet) -> Vec<Unverified> {
        set.changes()
            .iter()
            .flat_map(|change| {
                // The texts of a description the AI wrote are its text too
                // (D-729): the titles of the chart, its axes and legends.
                let (title, description) = match change {
                    ProposedChange::NewView(view) => (Some(&view.title), view.description()),
                    // A new title of a saved chart is the model's text too
                    // (D-681).
                    ProposedChange::EditView(view) => (
                        (view.title != view.before).then_some(&view.title),
                        view.description().filter(|_| view.writes_description()),
                    ),
                    _ => (None, None),
                };
                title
                    .cloned()
                    .into_iter()
                    .chain(description.map(description_titles).unwrap_or_default())
                    .flat_map(|text| check_numbers(&text, &self.figures))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The overview for a model that can't call functions, with its figures
    /// added to those the answer may name.
    fn overview(&mut self, book: &Book, today: Date) -> Result<Value, ChatError> {
        let output = kassensturz_ai_tools::overview_allowed(book, today, self.permissions())
            .map_err(ChatError::Book)?;
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
        kassensturz_ai_tools::call_allowed(book, name, arguments, today, self.permissions())
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

    /// Moves between the functions and the overview by what the connection
    /// test found (D-324), before the question's first request.
    fn follow_the_test(&mut self, hooks: &mut dyn Hooks) {
        if self.test_found_no_functions && self.mode == Mode::Functions && !self.functions_taken {
            self.mode = Mode::Overview;
            self.overview_from_test = true;
            self.note(LogEntry::Overview, hooks);
        } else if !self.test_found_no_functions && self.overview_from_test {
            self.mode = Mode::Functions;
            self.overview_from_test = false;
        }
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
    let mut specs: Vec<FunctionSpec> = readable_functions(permissions).map(spec).collect();
    specs.extend(propose_spec(permissions).map(|propose| FunctionSpec {
        name: propose.name.to_owned(),
        description: propose.description,
        parameters:
            serde_json::from_str(&propose.parameters).unwrap_or_else(|_| json!({"type": "object"})),
    }));
    specs.extend(propose_view_spec(permissions).map(|view| FunctionSpec {
        name: view.name.to_owned(),
        description: view.description,
        parameters:
            serde_json::from_str(&view.parameters).unwrap_or_else(|_| json!({"type": "object"})),
    }));
    specs.push(spec(&DRAFT_ISSUE));
    specs.push(spec(&OPEN_PAGE));
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

/// What the model is told when a name is taken (M6-B191): the name of a new
/// Rule must be free after the call's own deletions and renames.
const NAME_TAKEN_HINT: &str = "Names must be new among their neighbours, ignoring capitals. \
To replace a Rule by a new one of the same name, delete the old one with delete_rules in the \
same call; to rename one, use edit_rules.";

/// What the model is told when it wanted to rewrite conditions it cannot see
/// in full (M6-B191).
const CONDITIONS_HIDDEN_HINT: &str = "This Rule has conditions you cannot write (such as an \
IBAN). Leave conditions out of this edit_rules item to keep them, use add_conditions or \
remove_conditions to change single ones, or delete the Rule and make a new one.";

/// What the model is told when a call to `propose_changes` failed.
const NOT_PREPARED_HINT: &str = "Nothing was prepared. Do not tell the user that a change is \
prepared. Fix the call and call propose_changes again, or say plainly that it did not work.";

/// What the model is told when a call to `open_page` failed (M5-B41): once
/// a model claimed the page was open after its only call had failed.
const NOT_OPENED_HINT: &str = "The page was NOT opened. Do not tell the user that it is \
open. Fix the call and call open_page again, or say plainly that you could not open it.";

/// What the model is told when something is not allowed (M5-B44): where the
/// user can allow it, so it points them there instead of saying "do it by
/// hand".
const NOT_ALLOWED_HINT: &str = "This is switched off. Tell the user they can allow it in \
KassenSturz's AI settings, under \"What the AI may do\". Don't suggest doing it by hand.";

/// `answer` with `hint` added to its hints.
fn with_hint(mut answer: Value, hint: &str) -> Value {
    if let Value::Object(object) = &mut answer {
        let hints = match object.get("hint").and_then(Value::as_str) {
            Some(first) => format!("{first} {hint}"),
            None => hint.to_owned(),
        };
        object.insert("hint".into(), json!(hints));
    }
    answer
}

/// A function's error for the model: its code and the fixed names it
/// carries, never anything the model sent (D-034, X-8), and fixed hints
/// when an ID was wrong, a key was used for an ID, or the call that failed
/// was to prepare changes (`prepares`).
fn error_json(report: &ErrorReport, prepares: bool) -> Value {
    let mut out = Map::new();
    out.insert("error".into(), json!(report.code));
    let mut wrong_id =
        report.code.starts_with("ai-tools.unknown-") && report.code != "ai-tools.unknown-argument";
    let mut hints = Vec::new();
    for (name, value) in &report.params {
        match value {
            ErrorParam::Keyword(keyword) => {
                // An argument the call may not send is not a wrong ID (#580).
                wrong_id |= *name == "argument"
                    && keyword.ends_with("id")
                    && report.code != "ai-tools.unknown-argument";
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
            // The category a budget refusal is about, by the ID the model
            // has from list_categories (#830).
            ErrorParam::Account(id) if *name == "category_id" => {
                out.insert((*name).into(), json!(id.to_string()));
            }
            _ => {}
        }
    }
    if report.code == "ai-tools.name-taken" {
        hints.push(NAME_TAKEN_HINT);
    }
    if report.code == "ai-tools.not-changeable"
        && report.param("reason") == Some(&ErrorParam::Keyword("conditions-hidden"))
    {
        hints.push(CONDITIONS_HIDDEN_HINT);
    }
    if wrong_id {
        hints.push(ID_HINT);
    }
    if report.code.ends_with(".not-allowed") {
        hints.push(NOT_ALLOWED_HINT);
    }
    if prepares {
        hints.push(NOT_PREPARED_HINT);
    }
    if !hints.is_empty() {
        out.insert("hint".into(), json!(hints.join(" ")));
    }
    Value::Object(out)
}

/// `answer` with what the model can use instead: the keys an object
/// takes, when the call gave it one it doesn't take (M6-B201), or the words
/// that suit the chart, or the argument to use, when a chart setting
/// doesn't suit it (M6-B204). From KassenSturz's fixed lists, never the
/// model's own text (X-8). The error's own list replaces the general one of
/// the argument that [`view_error_json`] gives.
fn with_allowed(mut answer: Value, error: &ToolError) -> Value {
    if let Value::Object(object) = &mut answer {
        if let Some(allowed) = error.allowed_fields() {
            object.insert("allowed".into(), json!(allowed));
        }
        if let Some(argument) = error.use_instead() {
            object.insert("use_instead".into(), json!(argument));
        }
        // A split whose parts don't add up: by how much (D-737).
        for (name, amount) in error.amounts().into_iter().flatten() {
            object.insert(name.into(), json!(amount));
        }
        // Where the user can switch on what is off (D-676).
        if let Some(setting) = error.setting() {
            object.insert("setting".into(), setting.json());
        }
        if let Some(shape) = error.expected_shape() {
            object.insert("expected_shape".into(), json!(shape));
        }
        if let Some(ways) = error.only_for_show_as() {
            // A general list of the argument's words would mislead: the
            // chosen way of drawing takes none of them.
            object.remove("allowed");
            object.insert("only_for_show_as".into(), json!(ways));
        }
    }
    // E.g. a ready-made tile asked for as a chart (R2-16).
    match error.hint() {
        Some(hint) => with_hint(answer, hint),
        None => answer,
    }
}

/// The dataset a refused call of `propose_view` named, when it is one the AI
/// may name: the catalog's own id, never the model's text (D-402).
fn understood_dataset(arguments: &str) -> Option<&'static str> {
    let named = serde_json::from_str::<Value>(arguments).ok()?;
    let named = named.get("data")?.as_str()?;
    view_choices("data")?.into_iter().find(|id| *id == named)
}

/// What the model is told when its call of `propose_view` is refused: the
/// code, the argument by its fixed name and, for a list, the values it
/// takes from the catalog and schema (D-353). Never the model's own text.
fn view_error_json(report: &ErrorReport) -> Value {
    let mut out = error_json(report, true);
    let argument = report.params.iter().find_map(|(name, value)| match value {
        ErrorParam::Keyword(keyword) if *name == "argument" => Some(*keyword),
        _ => None,
    });
    if report.code == "ai-tools.bad-argument"
        && let Some(words) = argument.and_then(view_choices)
        && let Value::Object(object) = &mut out
    {
        object.insert("allowed".into(), json!(words));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kassensturz_ai_tools::FUNCTIONS;

    /// The chat reads no bill, so it never attaches one (Q-506, D-739): not
    /// offered, and refused if a model tries, whatever the user allowed.
    #[test]
    fn the_chat_never_attaches_bills() {
        let session = ChatSession::new(crate::BuildInfo::new("0.1.0", "abc"))
            .with_permissions(Permissions::all());
        let permissions = session.permissions();
        assert!(!permissions.allows(ai_switch::Permission::AttachBills));
        assert!(permissions.allows(ai_switch::Permission::SplitTransactions));
        let specs = function_specs(false, permissions);
        assert!(specs.iter().all(|spec| spec.name != "bill_folder"));
        let propose = specs
            .iter()
            .find(|spec| spec.name == "propose_changes")
            .expect("offered");
        assert!(!propose.parameters.to_string().contains("attach_bills"));
    }

    #[test]
    fn an_unknown_key_lists_the_keys_its_object_takes() {
        let error = ToolError::UnknownField {
            argument: "new_accounts",
            allowed: &["name", "type", "currency"],
        }
        .at("new_accounts", 1);
        let answer = with_allowed(error_json(&error.model_report(), true), &error);
        assert_eq!(answer["error"], json!("ai-tools.unknown-field"));
        assert_eq!(answer["argument"], json!("new_accounts"));
        assert_eq!(answer["item"], json!(2));
        assert_eq!(answer["allowed"], json!(["name", "type", "currency"]));
        // The error's own list replaces a general one.
        let replaced = with_allowed(json!({"allowed": ["x"]}), &error);
        assert_eq!(replaced["allowed"], json!(["name", "type", "currency"]));
    }

    #[test]
    fn a_measure_refused_on_a_chart_says_ready_made_tiles_are_not_charts() {
        // R2-16: the model asked for "Where your money came from" as a
        // chart and had nothing plain to tell the user.
        let error = ToolError::NotForChart {
            argument: "measure",
            allowed: Vec::new(),
            use_instead: None,
            only_for_show_as: vec!["figure"],
        };
        let answer = with_allowed(error_json(&error.model_report(), false), &error);
        assert_eq!(answer["only_for_show_as"], json!(["figure"]));
        assert_eq!(
            answer["hint"],
            json!(kassensturz_ai_tools::READY_MADE_TILE_HINT)
        );
        // Added to the hints the answer already has.
        let prepared = with_allowed(error_json(&error.model_report(), true), &error);
        let hint = prepared["hint"].as_str().expect("hints");
        assert!(hint.starts_with(NOT_PREPARED_HINT), "{hint}");
        assert!(hint.ends_with(kassensturz_ai_tools::READY_MADE_TILE_HINT));
    }

    #[test]
    fn a_refusal_for_something_switched_off_names_the_setting() {
        // Where the user allows it, in the app's two languages (D-676).
        for error in [
            ToolError::NotAllowed {
                argument: "delete_rules",
            },
            ToolError::FieldLocked { field: "payee" }.at("edit_transactions", 0),
        ] {
            let answer = with_allowed(error_json(&error.model_report(), true), &error);
            assert_eq!(
                answer["setting"],
                kassensturz_ai_tools::AI_PERMISSIONS.json()
            );
        }
        let other = ToolError::UnknownRule;
        let answer = with_allowed(error_json(&other.model_report(), true), &other);
        assert!(answer.get("setting").is_none(), "{answer}");
    }

    #[test]
    fn a_refused_budget_and_a_refused_text_carry_their_fixed_words() {
        // #830: the category by its ID (never a name); #826: the reason.
        let category = kassensturz_core::AccountId::generate();
        let budget = ToolError::HasBudget {
            argument: "category_ids",
            place: Some(2),
            category,
        }
        .report();
        let answer = error_json(&budget, true);
        assert_eq!(answer["category_id"], json!(category.to_string()));
        assert_eq!(answer["category"], json!(2));
        assert_eq!(answer["use_instead"], json!("edit_budgets"));
        let note = ToolError::BadText {
            argument: "note",
            reason: "line-break",
        }
        .report();
        let answer = error_json(&note, true);
        assert_eq!(answer["error"], json!("ai-tools.bad-argument"));
        assert_eq!(answer["argument"], json!("note"));
        assert_eq!(answer["reason"], json!("line-break"));
    }

    #[test]
    fn a_refused_chart_lists_what_its_argument_takes() {
        let bad = ToolError::BadArgument {
            argument: "show_as",
        }
        .report();
        let answer = view_error_json(&bad);
        assert_eq!(answer["argument"], json!("show_as"));
        assert_eq!(answer["allowed"], json!(view_choices("show_as").unwrap()));
        assert!(answer["hint"].as_str().unwrap().contains(NOT_PREPARED_HINT));
        // An argument that is no list, and other codes, list nothing.
        let title = ToolError::BadArgument { argument: "title" }.report();
        assert!(view_error_json(&title).get("allowed").is_none());
        assert!(
            view_error_json(
                &ToolError::UnknownArgument {
                    argument: None,
                    allowed: Vec::new(),
                }
                .report()
            )
            .get("allowed")
            .is_none()
        );
    }

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
        let everything = ai_switch::Field::ALL
            .iter()
            .fold(Permissions::all(), |p, field| p.with_lock(*field, false));
        let specs = function_specs(false, everything);
        assert_eq!(specs.len(), FUNCTIONS.len() + 4);
        assert_eq!(specs[FUNCTIONS.len()].name, PROPOSE_CHANGES.name);
        // A chart has its own function, offered with Charts › Add (D-352).
        let all_specs = specs.clone();
        let mut specs = specs;
        let view = specs.remove(FUNCTIONS.len() + 1);
        assert_eq!(view.name, PROPOSE_VIEW_NAME);
        assert_eq!(
            view.parameters,
            serde_json::from_str::<Value>(&propose_view_spec(everything).unwrap().parameters)
                .unwrap()
        );
        assert_eq!(specs[FUNCTIONS.len() + 1].name, DRAFT_ISSUE.name);
        assert_eq!(specs[FUNCTIONS.len() + 2].name, OPEN_PAGE.name);
        for (spec, function) in
            specs.iter().zip(
                FUNCTIONS
                    .iter()
                    .chain([&PROPOSE_CHANGES, &DRAFT_ISSUE, &OPEN_PAGE]),
            )
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
        let with_log = function_specs(true, everything);
        #[cfg(feature = "debug-log")]
        {
            assert_eq!(with_log.len(), all_specs.len() + 1);
            assert_eq!(with_log[all_specs.len()].name, READ_DEBUG_LOG.name);
            assert_eq!(
                with_log[all_specs.len()].parameters,
                serde_json::from_str::<Value>(READ_DEBUG_LOG.parameters).unwrap()
            );
        }
        #[cfg(not(feature = "debug-log"))]
        assert_eq!(with_log.len(), all_specs.len());
        assert_eq!(with_log[..all_specs.len()], all_specs[..]);
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
    fn a_refusal_points_to_the_settings_and_a_failed_page_is_not_opened() {
        let refused = ToolError::NotAllowed { argument: "page" }.report();
        let hint = error_json(&refused, false)["hint"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(hint.contains("What the AI may do"), "{hint}");
        // The code of a refused change set carries the same hint.
        let change = kassensturz_ai_tools::ChangeError::NotAllowed.report();
        assert_eq!(change.code, "ai-changes.not-allowed");
        assert!(
            error_json(&change, true)["hint"]
                .as_str()
                .unwrap()
                .contains("What the AI may do")
        );
        let failed = with_hint(
            error_json(
                &ToolError::UnknownArgument {
                    argument: None,
                    allowed: Vec::new(),
                }
                .report(),
                false,
            ),
            NOT_OPENED_HINT,
        );
        assert!(failed["hint"].as_str().unwrap().contains("NOT opened"));
        // Hints add up: a refused page says both.
        let both = with_hint(error_json(&refused, false), NOT_OPENED_HINT);
        let both = both["hint"].as_str().unwrap();
        assert!(both.contains("What the AI may do") && both.contains("NOT opened"));
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
    fn a_refused_item_is_named_with_its_list_its_place_and_a_hint_that_helps() {
        // The third of `new_rules`, whose name is taken (M6-B191).
        let report = ToolError::NameTaken.at("new_rules", 2).model_report();
        let answer = error_json(&report, true);
        assert_eq!(answer["error"], json!("ai-tools.name-taken"));
        assert_eq!(answer["list"], json!("new_rules"));
        assert_eq!(answer["item"], json!(3));
        let hint = answer["hint"].as_str().unwrap();
        assert!(hint.contains(NAME_TAKEN_HINT) && hint.contains(NOT_PREPARED_HINT));
        // A Rule with conditions the model can't write says which and why.
        let report = ToolError::NotChangeable {
            argument: "conditions",
            reason: "conditions-hidden",
        }
        .at("edit_rules", 0)
        .model_report();
        let answer = error_json(&report, false);
        assert_eq!(answer["argument"], json!("conditions"));
        assert_eq!(answer["reason"], json!("conditions-hidden"));
        assert_eq!(answer["item"], json!(1));
        assert_eq!(answer["hint"], json!(CONDITIONS_HIDDEN_HINT));
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
