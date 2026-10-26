// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Issue drafts (D-173, D-228, threat T-33): a model fills in a fixed
//! template for a bug report, KassenSturz adds the build, the screen and
//! the error codes, and the user reads the exact text before anything
//! happens. KassenSturz sends nothing itself: the front end opens a
//! prefilled issue page in the user's own browser, where the user submits
//! it or not (AGENTS.md §3.1 rule 4).
//!
//! The model's words are scanned first. A draft is refused when they hold
//! text from the book (a name of an account, category, payee, institution
//! or Rule, a payee, memo or bank text of a transaction), anything shaped
//! like a figure or an account number (four digits in a row, a decimal
//! number, an amount with a currency) or an e-mail address
//! (`ai-tools.issue-book-data`), or anything shaped like a secret, such as
//! a long run of letters and digits (`ai-tools.issue-secret`). The scan
//! errs on the side of refusing: the model can say the same in general
//! words.

use std::collections::BTreeSet;

use kassensturz_core::RuleNode;
use kassensturz_store::Book;
use regex::Regex;
use serde_json::{Value, json};

use crate::error::ToolError;
use crate::figures::Figures;
use crate::functions::{Function, Output};

/// The page a new issue is opened on. Its address is KassenSturz's own,
/// fixed in the program, never taken from a model.
pub const ISSUE_PAGE: &str = "https://github.com/Vondor/KassenSturz/issues/new";

/// The longest link the prefilled page gets. A longer draft goes with its
/// title only, and the user copies the text (D-173).
pub const MAX_LINK_BYTES: usize = 8_000;

/// The function that prepares an issue draft.
pub const DRAFT_ISSUE: Function = Function {
    name: "draft_issue",
    description: "Prepares a bug report about KassenSturz for the user to read, when the user \
        asks for one or agrees to one after you said something may be a bug. Write it in \
        English and in general words, e.g. \"a balance\", \"a payee\", \"a category\": never a \
        figure, a date, a name, a payee, an account, a category, a memo, an IBAN, an e-mail \
        address, a password, a PIN, a TAN or a key from the user or the book. KassenSturz \
        refuses a draft that holds one (ai-tools.issue-book-data, ai-tools.issue-secret, with \
        the field). KassenSturz adds the version, the commit, the screen and the error codes \
        itself. Nothing is sent: the user reads the draft and decides whether to open the \
        prefilled issue page in their own browser.",
    parameters: r#"{"type":"object","properties":{"title":{"type":"string","maxLength":100,"description":"A short summary of the problem."},"what_happened":{"type":"string","maxLength":1000,"description":"What happened, in general words."},"expected":{"type":"string","maxLength":500,"description":"What the user expected instead."},"steps":{"type":"array","maxItems":10,"items":{"type":"string","maxLength":200},"description":"The steps that led to it, one per item."}},"required":["title","what_happened"],"additionalProperties":false}"#,
};

/// What KassenSturz adds to a draft itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IssueContext {
    /// The build's version (D-152).
    pub version: String,
    /// The commit it was built from, empty when unknown.
    pub commit: String,
    /// The screen the user was on, from the fixed list of
    /// `kassensturz_core::vocabulary`.
    pub screen: Option<&'static str>,
    /// The error codes of the conversation and of the debug log, while AI
    /// may read it, in the order they came, each once.
    pub codes: Vec<String>,
    /// Whether the codes include the debug log's, read while the developer
    /// mode is on (D-227).
    pub log_read: bool,
}

/// A draft the user looks at before anything happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueDraft {
    /// The issue's title.
    pub title: String,
    /// The issue's text, exactly as the page gets it.
    pub body: String,
    /// The prefilled issue page: title and text, or the title only when
    /// the whole draft would make the link too long.
    pub link: String,
    /// Whether the link holds the text too; when it doesn't, the user
    /// copies it.
    pub whole_in_link: bool,
}

/// The arguments of a call, by their fixed names: the keywords of
/// `ai-tools.bad-argument`, `ai-tools.issue-book-data` and
/// `ai-tools.issue-secret`.
pub(crate) const FIELDS: &[&str] = &["title", "what_happened", "expected", "steps"];

/// The model's words, read from its call.
struct Words {
    title: String,
    what_happened: String,
    expected: Option<String>,
    steps: Vec<String>,
}

/// Prepares a draft from a model's call of [`DRAFT_ISSUE`] with
/// `arguments`, after the scan against `book`, and the result the model
/// gets, which tells it that the user now decides.
pub fn draft_issue(
    book: &Book,
    arguments: &Value,
    context: &IssueContext,
) -> Result<(IssueDraft, Output), ToolError> {
    let words = read(arguments)?;
    let book_texts = book_texts(book)?;
    scan("title", &words.title, &book_texts)?;
    scan("what_happened", &words.what_happened, &book_texts)?;
    if let Some(expected) = &words.expected {
        scan("expected", expected, &book_texts)?;
    }
    for step in &words.steps {
        scan("steps", step, &book_texts)?;
    }
    let draft = build(&words, context);
    let output = Output::new(
        json!({
            "drafted": true,
            "next": "The user now reads the draft in KassenSturz and decides whether to open \
                the prefilled issue page in their browser. Nothing was sent. Tell the user that.",
        }),
        Figures::new(),
    );
    Ok((draft, output))
}

fn read(arguments: &Value) -> Result<Words, ToolError> {
    let bad = |argument| ToolError::BadArgument { argument };
    let Value::Object(values) = arguments else {
        return Err(bad("arguments"));
    };
    if values.keys().any(|key| !FIELDS.contains(&key.as_str())) {
        return Err(bad("arguments"));
    }
    let text = |name: &'static str, max: usize| -> Result<Option<String>, ToolError> {
        match values.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) if text.chars().count() <= max => {
                Ok(Some(one_line_breaks(text)))
            }
            Some(_) => Err(bad(name)),
        }
    };
    let title = text("title", 100)?
        .filter(|title| !title.trim().is_empty())
        .ok_or(bad("title"))?;
    let what_happened = text("what_happened", 1000)?
        .filter(|text| !text.trim().is_empty())
        .ok_or(bad("what_happened"))?;
    let expected = text("expected", 500)?.filter(|text| !text.trim().is_empty());
    let steps = match values.get("steps") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(steps)) if steps.len() <= 10 => steps
            .iter()
            .map(|step| match step {
                Value::String(step) if step.chars().count() <= 200 => Ok(one_line_breaks(step)),
                _ => Err(bad("steps")),
            })
            .filter(|step| !step.as_ref().is_ok_and(|step| step.trim().is_empty()))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(bad("steps")),
    };
    Ok(Words {
        title: title.replace('\n', " ").trim().to_owned(),
        what_happened: what_happened.trim().to_owned(),
        expected: expected.map(|text| text.trim().to_owned()),
        steps: steps
            .into_iter()
            .map(|step| step.replace('\n', " ").trim().to_owned())
            .collect(),
    })
}

/// `text` without control and direction characters, with its line breaks
/// kept as plain `\n`.
fn one_line_breaks(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter(|c| {
            *c == '\n'
                || !(c.is_control()
                    || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'))
        })
        .collect()
}

/// Every text of the book a draft mustn't hold, in lowercase, at least three
/// characters long.
fn book_texts(book: &Book) -> Result<BTreeSet<String>, ToolError> {
    let mut texts = BTreeSet::new();
    let mut add = |text: &str| {
        let text = text.trim().to_lowercase();
        if text.chars().count() >= 3 {
            texts.insert(text);
        }
    };
    for account in book.all_accounts()? {
        add(&account.name);
    }
    for payee in book.payees()? {
        add(&payee.name);
    }
    for institution in book.institutions()? {
        add(&institution.name);
    }
    fn rules(nodes: &[RuleNode], add: &mut dyn FnMut(&str)) {
        for node in nodes {
            add(&node.rule.name);
            rules(&node.sub_rules, add);
        }
    }
    rules(&book.rules()?, &mut add);
    for transaction in book.transactions()? {
        transaction.payee().into_iter().for_each(&mut add);
        transaction.note().into_iter().for_each(&mut add);
        for part in transaction.splits() {
            part.own_payee.as_deref().into_iter().for_each(&mut add);
            part.note.as_deref().into_iter().for_each(&mut add);
            if let Some(imported) = &part.imported {
                imported
                    .other_party_name
                    .as_deref()
                    .into_iter()
                    .for_each(&mut add);
                imported
                    .remittance_information
                    .as_deref()
                    .into_iter()
                    .for_each(&mut add);
            }
        }
    }
    Ok(texts)
}

/// Refuses `text`, the model's words for `field`, when it holds book data or
/// something shaped like a secret.
fn scan(field: &'static str, text: &str, book_texts: &BTreeSet<String>) -> Result<(), ToolError> {
    static SHAPES: std::sync::OnceLock<(Regex, Regex)> = std::sync::OnceLock::new();
    let (figures, secrets) = SHAPES.get_or_init(|| {
        (
            // Four digits in a row, a decimal number, an amount with a
            // currency sign or code, an e-mail address.
            Regex::new(
                r"[0-9]{4}|[0-9][.,][0-9]|[0-9]\s*[€$£¥]|[€$£¥]\s*[0-9]|[0-9]\s*[A-Z]{3}\b|[^\s@]+@[^\s@]+\.[^\s@]+",
            )
            .expect("the figure pattern is valid"),
            // A run of letters and digits of at least 16 characters that
            // has both, a private key's armour, a bearer token.
            Regex::new(r"(?i)-----BEGIN|\bbearer\s+\S|\b(sk|ghp|gho|xox[a-z])[-_][A-Za-z0-9]")
                .expect("the secret pattern is valid"),
        )
    });
    let long_token = text
        .split(|c: char| !(c.is_ascii_alphanumeric() || "+/=_-".contains(c)))
        .any(|token| {
            token.len() >= 16
                && token.chars().any(|c| c.is_ascii_digit())
                && token.chars().any(|c| c.is_ascii_alphabetic())
        });
    if long_token || secrets.is_match(text) {
        return Err(ToolError::IssueSecret { field });
    }
    if figures.is_match(text) {
        return Err(ToolError::IssueBookData { field });
    }
    let lower = text.to_lowercase();
    if book_texts.iter().any(|book| holds_word(&lower, book)) {
        return Err(ToolError::IssueBookData { field });
    }
    Ok(())
}

/// Whether `text` holds `word` as a whole word or phrase, not inside a
/// longer word.
fn holds_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(start, found)| {
        let before = text[..start].chars().next_back();
        let after = text[start + found.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// The repository's bug report form, `.github/ISSUE_TEMPLATE/bug_report.yml`.
/// Blank issues are off there, so the link names the form and fills its
/// fields by their IDs; a link with only a body would show the form chooser
/// and lose the text.
const FORM: &str = "bug_report.yml";

/// The form's fields the draft fills, by ID and label, in the form's order.
const FORM_FIELDS: [(&str, &str); 3] = [
    ("what-happened", "What happened?"),
    ("steps", "Steps to reproduce"),
    ("version", "KassenSturz version"),
];

/// The draft in its fixed template: the form's fields, which the body
/// shows as the issue will show them.
fn build(words: &Words, context: &IssueContext) -> IssueDraft {
    let mut what = words.what_happened.clone();
    if let Some(expected) = &words.expected {
        what.push_str("\n\n**What I expected:** ");
        what.push_str(expected);
    }
    what.push_str("\n\n**Screen:** ");
    what.push_str(context.screen.unwrap_or("not known"));
    what.push_str("\n\n**Error codes:** ");
    if context.codes.is_empty() {
        what.push_str("none");
    } else {
        what.push_str(
            &context
                .codes
                .iter()
                .map(|code| format!("`{code}`"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if context.log_read {
        what.push_str(" (with the debug log)");
    }
    what.push_str(
        "\n\nDrafted by an AI in KassenSturz's chat; the user read it before opening this page.",
    );
    let steps = words
        .steps
        .iter()
        .enumerate()
        .map(|(number, step)| format!("{}. {step}", number + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let version = if context.commit.is_empty() {
        context.version.clone()
    } else {
        format!("{} (commit {})", context.version, context.commit)
    };
    let values = [what, steps, version];
    let body = FORM_FIELDS
        .iter()
        .zip(&values)
        .map(|((_, label), value)| {
            let value = if value.is_empty() {
                "(to be added on the page)"
            } else {
                value
            };
            format!("### {label}\n\n{value}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let title = words.title.clone();
    let start = format!("{ISSUE_PAGE}?template={FORM}&title={}", encode(&title));
    let field = |index: usize| format!("&{}={}", FORM_FIELDS[index].0, encode(&values[index]));
    let whole = format!("{start}{}{}{}", field(0), field(1), field(2));
    let (link, whole_in_link) = if whole.len() <= MAX_LINK_BYTES {
        (whole, true)
    } else {
        (format!("{start}{}", field(2)), false)
    };
    IssueDraft {
        title,
        body,
        link,
        whole_in_link,
    }
}

/// `text` for a link's query: every byte but letters, digits and `-._~`
/// written as `%XX`.
fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The error codes in `codes`, each once, in the order they first came,
/// only those shaped like codes.
pub fn unique_codes<'a>(codes: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    codes
        .into_iter()
        .filter(|code| kassensturz_core::is_error_code(code))
        .filter(|code| seen.insert(code.to_string()))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(words: &[&str]) -> BTreeSet<String> {
        words.iter().map(|word| word.to_lowercase()).collect()
    }

    fn context() -> IssueContext {
        IssueContext {
            version: "0.261025.4".into(),
            commit: "abc1234".into(),
            screen: Some("OverviewPage"),
            codes: vec!["ai.timeout".into()],
            log_read: true,
        }
    }

    fn words(what: &str) -> Words {
        Words {
            title: "The balance looks wrong".into(),
            what_happened: what.into(),
            expected: Some("The balance I see in the account".into()),
            steps: vec!["Open the overview".into(), "Ask about a balance".into()],
        }
    }

    #[test]
    fn the_template_holds_the_words_the_build_the_screen_and_the_codes() {
        let draft = build(&words("The chat said a function failed."), &context());
        assert_eq!(draft.title, "The balance looks wrong");
        let what = "The chat said a function failed.\n\n\
             **What I expected:** The balance I see in the account\n\n\
             **Screen:** OverviewPage\n\n\
             **Error codes:** `ai.timeout` (with the debug log)\n\n\
             Drafted by an AI in KassenSturz's chat; the user read it before opening this page.";
        assert_eq!(
            draft.body,
            format!(
                "### What happened?\n\n{what}\n\n\
                 ### Steps to reproduce\n\n1. Open the overview\n2. Ask about a balance\n\n\
                 ### KassenSturz version\n\n0.261025.4 (commit abc1234)"
            )
        );
        assert!(draft.whole_in_link);
        assert_eq!(
            draft.link,
            format!(
                "https://github.com/Vondor/KassenSturz/issues/new?template=bug_report.yml\
                 &title=The%20balance%20looks%20wrong&what-happened={}\
                 &steps=1.%20Open%20the%20overview%0A2.%20Ask%20about%20a%20balance\
                 &version=0.261025.4%20%28commit%20abc1234%29",
                encode(what)
            )
        );
        let unknown = IssueContext {
            commit: String::new(),
            screen: None,
            codes: Vec::new(),
            log_read: false,
            ..context()
        };
        let draft = build(
            &Words {
                steps: Vec::new(),
                ..words("x")
            },
            &unknown,
        );
        assert!(draft.body.contains("**Screen:** not known\n\n"));
        assert!(draft.body.contains("**Error codes:** none\n\n"));
        assert!(
            draft
                .body
                .contains("### Steps to reproduce\n\n(to be added on the page)\n\n")
        );
        assert!(
            draft
                .body
                .ends_with("### KassenSturz version\n\n0.261025.4")
        );
        assert!(draft.link.contains("&steps=&version=0.261025.4"));
    }

    #[test]
    fn the_link_fills_the_repositorys_bug_report_form() {
        let folder =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/ISSUE_TEMPLATE");
        let form = std::fs::read_to_string(folder.join(FORM)).expect("the bug report form");
        for (id, label) in FORM_FIELDS {
            assert!(form.contains(&format!("id: {id}\n")), "{id}");
            assert!(form.contains(&format!("label: {label}\n")), "{label}");
        }
        // Blank issues are off, which is why the link must name the form.
        let config = std::fs::read_to_string(folder.join("config.yml")).expect("config");
        assert!(config.contains("blank_issues_enabled: false"));
    }

    #[test]
    fn a_draft_too_long_for_a_link_goes_with_its_title_and_version_only() {
        let draft = build(&words(&"日".repeat(1000)), &context());
        assert!(!draft.whole_in_link);
        assert_eq!(
            draft.link,
            "https://github.com/Vondor/KassenSturz/issues/new?template=bug_report.yml\
             &title=The%20balance%20looks%20wrong&version=0.261025.4%20%28commit%20abc1234%29"
        );
        assert!(draft.body.contains(&"日".repeat(1000)));
    }

    #[test]
    fn book_data_and_secret_shapes_are_refused() {
        let book = texts(&["Moon Base Rent", "Oxygen", "Space Colony Bank"]);
        let refused_as_data = [
            "The rent to moon base rent shows twice",
            "OXYGEN is in the wrong category",
            "My balance is 1250 in the overview",
            "It showed 12,50",
            "It showed 3.5",
            "about 40 € went missing",
            "about € 40 went missing",
            "it said 40 EUR",
            "write to someone@example.org",
            "the IBAN DE89 3704 0044 0532 0130 00 shows",
        ];
        for text in refused_as_data {
            let error = scan("what_happened", text, &book).unwrap_err();
            assert!(
                matches!(
                    error,
                    ToolError::IssueBookData {
                        field: "what_happened"
                    }
                ),
                "{text}: {error:?}"
            );
        }
        let refused_as_secret = [
            "my key sk-abcdefghijklmnop leaked",
            "the key 0123456789abcdef0123 shows",
            "Bearer abc",
            "-----BEGIN PRIVATE KEY-----",
        ];
        for text in refused_as_secret {
            let error = scan("steps", text, &book).unwrap_err();
            assert!(
                matches!(error, ToolError::IssueSecret { field: "steps" }),
                "{text}: {error:?}"
            );
        }
        for text in [
            "The chat stopped after a function failed with ai.timeout",
            "Step 2 shows the oxygenated colours wrong",
            "A balance looked different in two places",
            "It happened in 2 of 3 tries",
        ] {
            assert!(scan("what_happened", text, &book).is_ok(), "{text}");
        }
    }

    #[test]
    fn calls_are_read_strictly() {
        let fine = json!({"title": "T", "what_happened": "W", "steps": ["a", "", "b"]});
        let taken = read(&fine).unwrap();
        assert_eq!(taken.steps, ["a", "b"]);
        assert_eq!(taken.expected, None);
        for (call, argument) in [
            (json!([]), "arguments"),
            (json!({"title": "T"}), "what_happened"),
            (json!({"what_happened": "W"}), "title"),
            (json!({"title": " ", "what_happened": "W"}), "title"),
            (
                json!({"title": "T", "what_happened": "W", "screen": "x"}),
                "arguments",
            ),
            (
                json!({"title": "x".repeat(101), "what_happened": "W"}),
                "title",
            ),
            (
                json!({"title": "T", "what_happened": "W", "steps": "a"}),
                "steps",
            ),
            (
                json!({"title": "T", "what_happened": "W", "steps": [1]}),
                "steps",
            ),
            (
                json!({"title": "T", "what_happened": "W", "steps": vec!["a"; 11]}),
                "steps",
            ),
        ] {
            match read(&call) {
                Err(ToolError::BadArgument { argument: found }) => {
                    assert_eq!(found, argument, "{call}");
                }
                _ => panic!("{call} was taken"),
            }
        }
        let hidden =
            read(&json!({"title": "A\u{202e}B\u{0007}", "what_happened": "x\r\ny"})).unwrap();
        assert_eq!(hidden.title, "AB");
        assert_eq!(hidden.what_happened, "x\ny");
    }

    #[test]
    fn codes_are_kept_once_in_order() {
        assert_eq!(
            unique_codes(["ai.timeout", "not a code", "ai-chat.stopped", "ai.timeout"]),
            ["ai.timeout", "ai-chat.stopped"]
        );
    }

    #[test]
    fn every_function_has_a_fixed_name_for_logs() {
        for function in crate::FUNCTIONS
            .iter()
            .chain([&crate::PROPOSE_CHANGES, &DRAFT_ISSUE])
        {
            assert!(
                kassensturz_core::vocabulary::known_detail(function.name).is_some(),
                "{}",
                function.name
            );
        }
        assert!(kassensturz_core::vocabulary::known_detail("read_debug_log").is_some());
    }

    #[test]
    fn links_encode_everything_but_the_plain_characters() {
        assert_eq!(encode("a b&c=d#ä~"), "a%20b%26c%3Dd%23%C3%A4~");
    }
}
