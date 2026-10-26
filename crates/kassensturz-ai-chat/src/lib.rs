// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! KassenSturz's AI chat (D-047, M5.4): one conversation with the model the
//! user set up, about one book.
//!
//! [`ChatSession::ask`] sends the user's question with KassenSturz's
//! instructions ([`BuildInfo`]: the build and that it is a pre-release,
//! D-174) and the fixed list of read functions (D-170). The model calls
//! functions, the chat runs them on the book and hands back their results,
//! until the model answers or [`MAX_ROUNDS`] requests were made (D-190). A
//! call for a function that wasn't offered is never run, and the model is
//! told so (threat T-31). A model that can't call functions gets the small
//! overview instead (D-052). The answer comes with every number in it that
//! no function produced ([`Answer::unverified`], D-188, threat T-26).
//!
//! Text from the book reaches the model only as the functions' JSON, whose
//! texts are cleaned (D-187) and can't close the block they stand in, and
//! the instructions say it is data, never an instruction (D-171, D-191,
//! threat T-21). What the model answers is untrusted text.
//!
//! Every step is in the tool-call log ([`LogEntry`], D-192): the question,
//! the exact body of each request, each function call with its result.
//! [`Hooks`] let the front end show each request before it goes and stop
//! the question ([`Cancel`], D-193).
//!
//! This crate joins `kassensturz-ai`, which holds the only way to a
//! provider and can't open a book (D-182, threat T-24), with
//! `kassensturz-ai-tools`, which reads the book and has no network code
//! (D-175). [`demo_provider`] is the fake model for trying the chat
//! without any AI (D-193).
//!
//! Without the `ai` feature this crate is empty and has no dependencies
//! (D-039).

#![cfg(feature = "ai")]
#![forbid(unsafe_code)]

mod demo;
mod error;
mod features;
mod log;
mod prompt;
mod session;

pub use demo::{DEMO_CATEGORY, DEMO_ENV, DEMO_LABEL, DemoModel, INVENTED, demo_provider};
pub use error::ChatError;
pub use features::{AnswerLanguage, Feature};
pub use log::{LogEntry, MAX_ARGUMENT_CHARS};
pub use prompt::{BuildInfo, DATA_END, DATA_START};
pub use session::{
    Answer, Cancel, ChatSession, Hooks, MAX_ROUNDS, Mode, NOT_OFFERED, NoHooks, Outcome,
    SHORTENED_RESULT, sample_request,
};
