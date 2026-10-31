// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Asks a small model on this computer for the ten described charts
//! (M6c.10b, D-356, D-412). Run by hand through
//! `tools/describe-charts-local.sh`, never in CI.
//!
//! Each description of [`DESCRIBED_CHARTS`] goes to the model as a new
//! question in a chat of its own, on the synthetic Space Colony book that
//! is in the repository (no other book can be named), with the grid
//! allowing only the reading of categories and accounts and preparing
//! charts. The chat, `propose_view` and its corrections are the app's own
//! (D-404). For each description it prints whether the first call of
//! `propose_view` was taken, how many refused calls came before one was
//! taken, whether the chart was prepared, the dataset named against the
//! fake model's, and the error codes of the refusals; then the share
//! prepared at the first try and after corrections. The output is a
//! Markdown table for the AI questionnaire (`ai-tests/` in the project's
//! shared folder); it holds fixed codes and numbers, never the model's
//! text.
//!
//! The address must be on this computer (a loopback address): the script
//! refuses any other, so nothing of the book leaves the machine.

use std::process::ExitCode;
use std::time::Instant;

#[path = "../tests/common/mod.rs"]
mod common;

use kassensturz_ai::{AiClient, ConfigFile, KeyUse, Place, ProviderAddress, ProviderSettings};
use kassensturz_ai_chat::{BuildInfo, ChatSession, DESCRIBED_CHARTS, LogEntry, NoHooks};
use kassensturz_ai_tools::PROPOSE_VIEW_NAME;
use kassensturz_core::Report;
use kassensturz_core::ai_switch::{Permission, Permissions};
use serde_json::Value;

const USAGE: &str = "usage: describe_charts_local --address http://127.0.0.1:1234/v1 --model NAME";

/// What happened to one description.
struct Outcome {
    /// The calls of `propose_view`: the dataset named and the error code.
    calls: Vec<(Option<String>, Option<&'static str>)>,
    prepared: bool,
    /// The chat's own error, if the question failed.
    failed: Option<&'static str>,
    seconds: u64,
}

impl Outcome {
    fn first_try(&self) -> bool {
        self.calls.first().is_some_and(|(_, error)| error.is_none())
    }

    fn refused(&self) -> usize {
        self.calls
            .iter()
            .take_while(|(_, error)| error.is_some())
            .count()
    }

    fn named(&self) -> Option<&str> {
        self.calls
            .iter()
            .find(|(_, error)| error.is_none())
            .and_then(|(data, _)| data.as_deref())
    }
}

fn main() -> ExitCode {
    let mut address = None;
    let mut model = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--address" => address = args.next(),
            "--model" => model = args.next(),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(address), Some(model)) = (address, model) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    // Nothing of the book may leave this computer (D-356).
    if !ProviderAddress::parse(&address).is_ok_and(|parsed| parsed.place() == Place::ThisComputer) {
        eprintln!(
            "describe_charts_local: only a model on this computer (127.0.0.1, ::1 or localhost) \
             is asked"
        );
        return ExitCode::from(2);
    }
    let Ok(settings) = ProviderSettings::new(&address, &model, KeyUse::None) else {
        eprintln!("describe_charts_local: the model name is not usable");
        return ExitCode::from(2);
    };
    let folder = std::env::temp_dir().join(format!(
        "kassensturz-describe-charts-{}",
        std::process::id()
    ));
    if std::fs::create_dir_all(&folder).is_err() {
        eprintln!("describe_charts_local: no temporary folder");
        return ExitCode::FAILURE;
    }
    let path = folder.join("kassensturzrc");
    let mut text = String::from("[AI]\nEnabled=true\n");
    for (key, value) in settings.entries() {
        text.push_str(&format!("{key}={value}\n"));
    }
    if std::fs::write(&path, text).is_err() {
        eprintln!("describe_charts_local: the settings file can't be written");
        return ExitCode::FAILURE;
    }
    let provider = AiClient::new(ConfigFile::at(&path));
    let book = common::space_colony();
    let allowed = Permissions::defaults()
        .with(Permission::CategoriesRead, true)
        .with(Permission::AccountsRead, true)
        .with(Permission::NewViews, true);

    let mut outcomes = Vec::new();
    for (number, chart) in DESCRIBED_CHARTS.iter().enumerate() {
        eprintln!(
            "{}/{}: {}",
            number + 1,
            DESCRIBED_CHARTS.len(),
            chart.description
        );
        let mut chat = ChatSession::new(BuildInfo::new(env!("CARGO_PKG_VERSION"), "local-test"))
            .with_permissions(allowed);
        let started = Instant::now();
        let answer = chat.ask(
            &book,
            &provider,
            None,
            chart.description,
            common::TODAY,
            &mut NoHooks,
        );
        let calls = chat
            .log()
            .iter()
            .filter_map(|entry| match entry {
                LogEntry::Called {
                    function,
                    arguments,
                    error,
                    ..
                } if function == PROPOSE_VIEW_NAME => {
                    let data = serde_json::from_str::<Value>(arguments)
                        .ok()
                        .and_then(|call| call["data"].as_str().map(str::to_owned));
                    Some((data, *error))
                }
                _ => None,
            })
            .collect();
        let (prepared, failed) = match &answer {
            Ok(answer) => (!answer.changes.is_empty(), None),
            Err(error) => (false, Some(error.report().code)),
        };
        outcomes.push(Outcome {
            calls,
            prepared,
            failed,
            seconds: started.elapsed().as_secs(),
        });
    }
    let _ = std::fs::remove_dir_all(&folder);
    print!("{}", report(&model, &outcomes));
    ExitCode::SUCCESS
}

/// The Markdown table and the two shares.
fn report(model: &str, outcomes: &[Outcome]) -> String {
    let mut out = format!(
        "### Describe a chart: {model}\n\n\
         | # | Description | First try | Refused before | Prepared | Dataset (expected) | \
         Refusals | Seconds |\n|---|---|---|---|---|---|---|---|\n"
    );
    for (number, (chart, outcome)) in DESCRIBED_CHARTS.iter().zip(outcomes).enumerate() {
        let expected = serde_json::from_str::<Value>(chart.call)
            .ok()
            .and_then(|call| call["data"].as_str().map(str::to_owned))
            .unwrap_or_default();
        let named = outcome.named().unwrap_or("none");
        let refusals: Vec<&str> = outcome
            .calls
            .iter()
            .filter_map(|(_, error)| *error)
            .chain(outcome.failed)
            .collect();
        let yes = |value: bool| if value { "yes" } else { "no" };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {named} ({expected}) | {} | {} |\n",
            number + 1,
            chart.description,
            yes(outcome.first_try()),
            outcome.refused(),
            yes(outcome.prepared),
            if refusals.is_empty() {
                "none".to_owned()
            } else {
                refusals.join(", ")
            },
            outcome.seconds,
        ));
    }
    let total = outcomes.len();
    let first = outcomes
        .iter()
        .filter(|outcome| outcome.first_try())
        .count();
    let prepared = outcomes.iter().filter(|outcome| outcome.prepared).count();
    let matching = DESCRIBED_CHARTS
        .iter()
        .zip(outcomes)
        .filter(|(chart, outcome)| {
            outcome.prepared
                && serde_json::from_str::<Value>(chart.call)
                    .ok()
                    .is_some_and(|call| outcome.named().is_some_and(|named| call["data"] == named))
        })
        .count();
    out.push_str(&format!(
        "\nTaken at the first try: {first} of {total}. Prepared after corrections: {prepared} \
         of {total}. Prepared with the expected dataset: {matching} of {total}.\n"
    ));
    out
}
