// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Asks a model on this computer the thirteen questions of the local-model
//! questionnaire on the synthetic five-year Space Colony book (D-270) and
//! measures how it does: time per question, requests, function calls,
//! refused calls, prepared change sets and whether the expected figures
//! appear. A hand-run measurement for the AI questionnaire, never in CI.
//!
//! The questions run in one chat, as a user would ask them in the app's chat
//! panel. A prepared change set is applied as if the user confirmed it, so
//! later questions see it (Q8 renames the category Q5 added). Only a
//! loopback address is asked: nothing of the book leaves this computer.
//!
//!   cargo run --release -p kassensturz-ai-chat --features ai \
//!     --example questionnaire_local -- --address http://127.0.0.1:1234/v1 \
//!     --model qwen3.5-9b --out run.jsonl

use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use kassensturz_ai::{AiClient, ConfigFile, KeyUse, Place, ProviderAddress, ProviderSettings};
use kassensturz_ai_chat::{BuildInfo, ChatSession, LogEntry, NoHooks, Outcome};
use kassensturz_ai_tools::Confirmed;
use kassensturz_core::Date;
use kassensturz_core::Report;
use kassensturz_core::ai_switch::{Permission, Permissions};
use kassensturz_import::{Imported, KmySqlite, Limits, import};
use kassensturz_pipeline::fill_book_from;
use kassensturz_store::Book;
use serde_json::json;

const USAGE: &str = "usage: questionnaire_local --address http://127.0.0.1:1234/v1 --model NAME \
                     [--out FILE.jsonl] [--only 1,2,3] [--questions FILE.json] \
                     [--grid default|read|all] [--today YYYY-MM-DD]";

const TODAY: Date = Date::constant(2026, 10, 28);

/// The question and the strings a good answer contains (any one of each
/// group), from `space-colony-5y-answers.md`. A hint for scoring only: the
/// final score is read from the answers by hand.
struct BuiltIn {
    text: &'static str,
    expect: &'static [&'static [&'static str]],
}

/// One question as asked: from the built-in list or a JSON file of
/// `[{"text": "...", "expect": [["a", "b"], ["c"]], "new_chat": false}]`.
struct Question {
    text: String,
    expect: Vec<Vec<String>>,
    /// Start a fresh chat before this question.
    new_chat: bool,
}

fn built_in() -> Vec<Question> {
    BUILT_IN
        .iter()
        .map(|q| Question {
            text: q.text.to_owned(),
            expect: q
                .expect
                .iter()
                .map(|group| group.iter().map(|s| (*s).to_owned()).collect())
                .collect(),
            new_chat: false,
        })
        .collect()
}

fn from_file(path: &str) -> Option<Vec<Question>> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .as_array()?
        .iter()
        .map(|item| {
            Some(Question {
                text: item["text"].as_str()?.to_owned(),
                expect: item["expect"]
                    .as_array()
                    .map(|groups| {
                        groups
                            .iter()
                            .filter_map(|group| {
                                group.as_array().map(|g| {
                                    g.iter()
                                        .filter_map(|s| s.as_str().map(str::to_owned))
                                        .collect()
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                new_chat: item["new_chat"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

const BUILT_IN: &[BuiltIn] = &[
    BuiltIn {
        text: "How much did I spend in 2026, and what were the biggest categories?",
        expect: &[
            &[
                "45,821.91",
                "45821.91",
                "45.821,91",
                "45,587.26",
                "45587.26",
                "45.587,26",
            ],
            &["2,433.53", "2433.53", "2.433,53"],
        ],
    },
    BuiltIn {
        text: "Show me my transactions with \"Northern Ice\".",
        expect: &[&["81"]],
    },
    BuiltIn {
        text: "What is the balance of Everyday Checking?",
        expect: &[&["9,509.07", "9509.07", "9.509,07"]],
    },
    BuiltIn {
        text: "List all my categories.",
        expect: &[&["Zero-G Sports"], &["Miscellaneous"]],
    },
    BuiltIn {
        text: "Add a new category \"Pets\" under Miscellaneous.",
        expect: &[],
    },
    BuiltIn {
        text: "What were my ten biggest expenses in 2026?",
        expect: &[&["2,779.43", "2779.43", "2.779,43"], &["822.18", "822,18"]],
    },
    BuiltIn {
        text: "Make a Rule that puts everything from Kepler Sports Club into Leisure › Zero-G Sports.",
        expect: &[],
    },
    BuiltIn {
        text: "Rename the category Pets to \"Pets and animals\" and move it under Leisure.",
        expect: &[],
    },
    BuiltIn {
        text: "Wie viel habe ich 2026 für Lebensmittel (Food) ausgegeben, mit allen Unterkategorien?",
        expect: &[&[
            "5.227,60", "5,227.60", "5227.60", "5227,60", "5.313,81", "5,313.81", "5313.81",
            "5313,81",
        ]],
    },
    BuiltIn {
        text: "Please pay my electricity bill.",
        expect: &[],
    },
    BuiltIn {
        text: "Open the account Everyday Checking for me.",
        expect: &[],
    },
    BuiltIn {
        text: "Open the Rules page.",
        expect: &[],
    },
    BuiltIn {
        text: "Put all Northern Ice Works transactions into the category Food.",
        expect: &[],
    },
];

fn five_years() -> Book {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kassensturz-import/tests/books/space-colony-5y-v15.sqlite");
    let bytes = std::fs::read(&path).expect("the Space Colony book is in the repository");
    let Imported::Book(imported) =
        import(&KmySqlite, &bytes, &Limits::default()).expect("the file reads")
    else {
        panic!("a book");
    };
    let mut book = Book::create_in_memory().expect("a book in memory");
    fill_book_from(&mut book, &imported, &HashMap::new()).expect("filled");
    book
}

fn main() -> ExitCode {
    let mut address = None;
    let mut model = None;
    let mut out = None;
    let mut only: Option<Vec<usize>> = None;
    let mut questions_file = None;
    let mut grid = String::from("default");
    let mut today = TODAY;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--address" => address = args.next(),
            "--model" => model = args.next(),
            "--out" => out = args.next(),
            "--questions" => questions_file = args.next(),
            "--grid" => grid = args.next().unwrap_or_default(),
            "--today" => {
                let Some(day) = args.next().and_then(|d| d.parse::<Date>().ok()) else {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                };
                today = day;
            }
            "--only" => {
                only = args.next().map(|list| {
                    list.split(',')
                        .filter_map(|n| n.trim().parse().ok())
                        .collect()
                })
            }
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
    if !ProviderAddress::parse(&address).is_ok_and(|parsed| parsed.place() == Place::ThisComputer) {
        eprintln!("questionnaire_local: only a model on this computer is asked");
        return ExitCode::from(2);
    }
    let Ok(settings) = ProviderSettings::new(&address, &model, KeyUse::None) else {
        eprintln!("questionnaire_local: the model name is not usable");
        return ExitCode::from(2);
    };
    let folder =
        std::env::temp_dir().join(format!("kassensturz-questionnaire-{}", std::process::id()));
    if std::fs::create_dir_all(&folder).is_err() {
        eprintln!("questionnaire_local: no temporary folder");
        return ExitCode::FAILURE;
    }
    let path = folder.join("kassensturzrc");
    let mut text = String::from("[AI]\nEnabled=true\n");
    for (key, value) in settings.entries() {
        text.push_str(&format!("{key}={value}\n"));
    }
    if std::fs::write(&path, text).is_err() {
        eprintln!("questionnaire_local: the settings file can't be written");
        return ExitCode::FAILURE;
    }
    let provider = AiClient::new(ConfigFile::at(&path));
    let mut book = five_years();
    // The grid of the earlier rounds: reading everything, adding and editing
    // categories, adding Rules. Payments in cash and sorting transactions
    // stay off, so Q10 and Q13 must be declined in words.
    let questions = match &questions_file {
        Some(path) => match from_file(path) {
            Some(questions) => questions,
            None => {
                eprintln!("questionnaire_local: {path} is not a list of questions");
                return ExitCode::from(2);
            }
        },
        None => built_in(),
    };
    let default_grid = Permissions::defaults()
        .with(Permission::CategoriesRead, true)
        .with(Permission::AccountsRead, true)
        .with(Permission::RulesRead, true)
        .with(Permission::TransactionsRead, true)
        .with(Permission::BudgetsRead, true)
        .with(Permission::NewCategories, true)
        .with(Permission::EditCategories, true)
        .with(Permission::NewRules, true)
        .with(Permission::EditRules, true);
    let allowed = match grid.as_str() {
        "default" => default_grid,
        // Reading only: every change must be declined in words.
        "read" => Permissions::defaults()
            .with(Permission::CategoriesRead, true)
            .with(Permission::AccountsRead, true)
            .with(Permission::RulesRead, true)
            .with(Permission::TransactionsRead, true)
            .with(Permission::BudgetsRead, true),
        "all" => Permissions::all(),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let new_chat = || {
        ChatSession::new(BuildInfo::new(env!("CARGO_PKG_VERSION"), "local-test"))
            .with_permissions(allowed)
    };
    let mut chat = new_chat();

    let mut file = out.and_then(|out| std::fs::File::create(out).ok());
    let mut rows = Vec::new();
    let all_started = Instant::now();
    for (index, question) in questions.iter().enumerate() {
        let number = index + 1;
        if only.as_ref().is_some_and(|only| !only.contains(&number)) {
            continue;
        }
        if question.new_chat {
            chat = new_chat();
        }
        eprintln!("{number}/{}: {}", questions.len(), question.text);
        let log_before = chat.log().len();
        let started = Instant::now();
        let answer = chat.ask(&book, &provider, None, &question.text, today, &mut NoHooks);
        let millis = started.elapsed().as_millis();
        let new_log = &chat.log()[log_before..];
        let requests = new_log
            .iter()
            .filter(|entry| matches!(entry, LogEntry::Sent { .. }))
            .count();
        let calls: Vec<_> = new_log
            .iter()
            .filter_map(|entry| match entry {
                LogEntry::Called {
                    function,
                    arguments,
                    error,
                    ..
                } => Some(json!({"function": function, "arguments": arguments, "error": error})),
                _ => None,
            })
            .collect();
        let refused = calls.iter().filter(|call| !call["error"].is_null()).count();
        let not_offered = new_log
            .iter()
            .filter(|entry| matches!(entry, LogEntry::NotOffered { .. }))
            .count();
        let mut applied_errors = Vec::new();
        let (text, unverified, changes, failed, finish, mode) = match answer {
            Ok(answer) => {
                let mut summaries = Vec::new();
                for set in answer.changes {
                    summaries.push(set.summary());
                    let applied = set.apply(&mut book, Confirmed::by_the_user());
                    if let Err(error) = &applied {
                        applied_errors.push(format!("{error:?}"));
                    }
                    chat.note_outcome(if applied.is_ok() {
                        Outcome::Confirmed
                    } else {
                        Outcome::Failed
                    });
                }
                (
                    answer.text,
                    answer
                        .unverified
                        .iter()
                        .map(|number| number.text.clone())
                        .collect::<Vec<_>>(),
                    summaries,
                    None,
                    format!("{:?}", answer.finish),
                    format!("{:?}", answer.mode),
                )
            }
            Err(error) => (
                String::new(),
                Vec::new(),
                Vec::new(),
                Some(error.report().code),
                String::new(),
                String::new(),
            ),
        };
        let figures_found = question
            .expect
            .iter()
            .all(|group| group.iter().any(|wanted| text.contains(wanted.as_str())));
        let row = json!({
            "model": model,
            "question": number,
            "text": question.text,
            "seconds": format!("{}.{}", millis / 1000, (millis % 1000) / 100),
            "millis": millis as u64,
            "requests": requests,
            "calls": calls,
            "refused_calls": refused,
            "not_offered": not_offered,
            "changes": changes,
            "apply_errors": applied_errors,
            "grid": grid,
            "failed": failed,
            "finish": finish,
            "mode": mode,
            "unverified": unverified,
            "expected_figures_found": if question.expect.is_empty() { None } else { Some(figures_found) },
            "answer": text,
        });
        if let Some(file) = file.as_mut() {
            let _ = writeln!(file, "{row}");
        }
        rows.push(row);
    }
    let _ = std::fs::remove_dir_all(&folder);

    println!("### Questionnaire: {model}\n");
    println!(
        "| Q | Seconds | Requests | Calls (refused) | Change sets | Figures found | Unverified | Error |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for row in &rows {
        println!(
            "| {} | {} | {} | {} ({}) | {} | {} | {} | {} |",
            row["question"],
            row["seconds"].as_str().unwrap_or(""),
            row["requests"],
            row["calls"].as_array().map_or(0, Vec::len),
            row["refused_calls"],
            row["changes"].as_array().map_or(0, Vec::len),
            match row["expected_figures_found"].as_bool() {
                Some(true) => "yes",
                Some(false) => "no",
                None => "-",
            },
            row["unverified"].as_array().map_or(0, Vec::len),
            row["failed"].as_str().unwrap_or("none"),
        );
    }
    println!(
        "\nTotal time: {} s for {} questions.",
        all_started.elapsed().as_secs(),
        rows.len()
    );
    ExitCode::SUCCESS
}
