// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The write tools (M5.10, D-035, D-217): the user's own AI tool prepares
//! changes, and the user confirms them in the KassenSturz window.
//!
//! They are offered only while the user has turned them on next to the AI
//! switch (`McpWriteTools`, D-219), and this program still never changes
//! the book: `propose_changes`, and `propose_view` for a chart (D-352),
//! check the call against the book like the chat's (D-199), then hand it to the running app as a request file
//! (D-216) and waits a while for the user's decision. The app checks the
//! call again itself, shows the set, and only its confirm button applies it
//! (D-200). `check_proposed_changes` asks again later about a set the user
//! hasn't decided on yet. With `check_only: true`, the two tools only check
//! the call and answer what it would prepare, and send nothing (D-674).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kassensturz_ai_tools::requests::{Reply, RequestFolder, RequestId, Standing};
use kassensturz_ai_tools::{
    CHECK_ONLY, CHECK_ONLY_SCHEMA, PROPOSE_CHANGES, PROPOSE_VIEW_NAME, ProposeSpec, ToolError,
    check_only_answer, prepare_named_with_bills, propose_spec, propose_view_spec, take_check_only,
};
use kassensturz_core::ai_switch::{self, Permissions};
use kassensturz_store::Book;
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Map, Value, json};

/// The tool that asks again about a request.
pub(crate) const CHECK_PROPOSED_CHANGES: &str = "check_proposed_changes";

/// What the MCP server puts at the start of the description of
/// `propose_changes` and `propose_view`. At the start, because clients cut
/// long descriptions at the end (M6-B201, US3.7).
const PROPOSE_NOTE: &str = "KassenSturz must be running with this book open. The answer's \
    status says whether the user applied the changes (applied), threw them away (declined) or \
    hasn't decided yet (waiting); for waiting, call check_proposed_changes with its request \
    later. Don't prepare the same changes again while they wait. Declined may list the \
    user's reasons: wrong_category, wrong_amount, wrong_transactions, too_many_changes, \
    not_what_was_asked. ";

const CHECK_DESCRIPTION: &str = "Asks whether the user has decided on changes that \
    propose_changes prepared and that were still waiting. Waits a while for the decision. The \
    answer's status is applied, declined or waiting. Declined may list reasons the user \
    picked from a fixed list: wrong_category (a category you chose is wrong), wrong_amount (an \
    amount is wrong), wrong_transactions (you picked the wrong transactions), too_many_changes \
    (too many changes at once), not_what_was_asked (not what the user asked for).";

const CHECK_PARAMETERS: &str = r#"{"type":"object","properties":{"request":{"type":"string","description":"The request from the answer of propose_changes."}},"required":["request"],"additionalProperties":false}"#;

/// How long a write tool waits, and how often it looks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Waits {
    /// How long the app has to take a request before the tool gives up:
    /// the app looks once a second while it runs (D-218).
    pub(crate) pickup: Duration,
    /// How long the tool waits for the user's decision before it answers
    /// "waiting": less than the minute after which AI tools commonly give
    /// up on a tool call.
    pub(crate) decision: Duration,
    /// How often it looks at the answer.
    pub(crate) step: Duration,
}

/// The waits of the running program.
pub(crate) const WAITS: Waits = Waits {
    pickup: Duration::from_secs(10),
    decision: Duration::from_secs(45),
    step: Duration::from_millis(250),
};

/// Why a write tool has no answer: an error code, with the report's
/// keywords when it came from the check of the call.
pub(crate) enum Failure {
    /// An error code: one of this module's, or the app's answer.
    Code(String),
    /// The check of the call against the book failed.
    Tool(ToolError),
}

/// The write tools, offered while they are turned on: `propose_changes`
/// with only the lists the user allows (D-239), `propose_view` while the
/// user allows Charts › Add (D-352), and nothing at all when they allow
/// none.
pub(crate) fn tools(permissions: Permissions) -> Vec<Tool> {
    let schema = |text: &str| -> Arc<JsonObject> {
        Arc::new(serde_json::from_str(text).expect("the parameters are a JSON object"))
    };
    let prepares = |spec: ProposeSpec| {
        // Only here, not in the chat: a check sends nothing to the app
        // (#595, D-674).
        let mut parameters = schema(&spec.parameters);
        if let Some(Value::Object(properties)) =
            Arc::make_mut(&mut parameters).get_mut("properties")
        {
            properties.insert(
                CHECK_ONLY.into(),
                serde_json::from_str(CHECK_ONLY_SCHEMA).expect("the check_only schema is JSON"),
            );
        }
        Tool::new(
            spec.name,
            format!("{PROPOSE_NOTE}{}", spec.description),
            parameters,
        )
        .with_annotations(
            ToolAnnotations::new()
                .read_only(false)
                .destructive(false)
                .idempotent(false)
                .open_world(false),
        )
    };
    let mut tools: Vec<Tool> = propose_spec(permissions)
        .into_iter()
        .chain(propose_view_spec(permissions))
        .map(prepares)
        .collect();
    if tools.is_empty() {
        return tools;
    }
    tools.push(
        Tool::new(
            CHECK_PROPOSED_CHANGES,
            CHECK_DESCRIPTION,
            schema(CHECK_PARAMETERS),
        )
        .with_annotations(
            ToolAnnotations::new()
                .read_only(true)
                .destructive(false)
                .idempotent(true)
                .open_world(false),
        ),
    );
    tools
}

/// `permissions` with Transactions › Attach bills off while no bill folder
/// is chosen (D-739): `attach_bills` is offered only with a folder.
pub(crate) fn with_bill_folder(permissions: Permissions, folder_chosen: bool) -> Permissions {
    if folder_chosen {
        permissions
    } else {
        permissions.with(ai_switch::Permission::AttachBills, false)
    }
}

/// Whether `name` is one of the write tools.
pub(crate) fn is_write_tool(name: &str) -> bool {
    name == PROPOSE_CHANGES.name || name == PROPOSE_VIEW_NAME || name == CHECK_PROPOSED_CHANGES
}

/// Runs the write tool `name`, with the request folder at `folder` (`None`
/// where this system has none).
pub(crate) async fn call(
    book: &Path,
    name: &str,
    arguments: &Value,
    folder: Option<&Path>,
    waits: Waits,
) -> Result<Value, Failure> {
    let bill_folder = ai_switch::bill_folder();
    let permissions = with_bill_folder(ai_switch::permissions(), bill_folder.is_some());
    if !ai_switch::write_tools_enabled() || !permissions.can_change() {
        return Err(Failure::Code("mcp-writes.off".into()));
    }
    let folder = folder
        .and_then(|path| RequestFolder::open(path.to_path_buf()).ok())
        .ok_or_else(|| Failure::Code("mcp-writes.folder".into()))?;
    let result = if name == PROPOSE_CHANGES.name || name == PROPOSE_VIEW_NAME {
        propose(
            book,
            name,
            arguments,
            permissions,
            bill_folder.as_deref(),
            &folder,
            waits,
        )
        .await
    } else {
        let id = request_of(arguments)?;
        wait_for(&folder, &id, waits, ai_switch::write_tools_enabled).await
    };
    // AI switched off while it waited is the kill switch: no answer at all
    // (D-013).
    crate::server::stop_if_switched_off();
    result
}

/// Checks the call against the book, hands it to the app and waits.
async fn propose(
    book: &Path,
    name: &str,
    arguments: &Value,
    permissions: Permissions,
    bill_folder: Option<&Path>,
    folder: &RequestFolder,
    waits: Waits,
) -> Result<Value, Failure> {
    // `check_only` is this server's own argument: the app gets the call
    // without it (D-674).
    let (check_only, arguments) = take_check_only(arguments).map_err(Failure::Tool)?;
    // The same check the app makes again (D-199): the AI tool hears about
    // a bad call at once, and nothing reaches the app that can't work.
    let (checked, figure) = Book::open_to_read(book)
        .map_err(ToolError::from)
        .and_then(|book| {
            book.reading(|book| {
                let proposal = prepare_named_with_bills(
                    book,
                    name,
                    &arguments,
                    permissions,
                    crate::server::today(),
                    bill_folder,
                )?;
                if check_only {
                    check_only_answer(book, &proposal, permissions)
                        .map(|answer| (Some(answer), Map::new()))
                } else {
                    Ok((None, figure_of(&proposal.output.value)))
                }
            })
        })
        .map_err(Failure::Tool)?;
    if let Some(answer) = checked {
        return Ok(answer);
    }
    let id = folder
        .send_call(book, name, &arguments)
        .map_err(|_| Failure::Code("mcp-writes.folder".into()))?;
    let mut answer = wait_for(folder, &id, waits, ai_switch::write_tools_enabled).await?;
    // The figure the prepared chart shows (D-685), with the answer that
    // says it was saved or still waits; a declined chart shows none.
    if answer["status"] != "declined"
        && let Value::Object(answer) = &mut answer
    {
        answer.extend(figure);
    }
    Ok(answer)
}

/// The figure a prepared key figure or progress bar shows, from the answer
/// of `propose_view` (D-685): `computed` and its note, or `not_computed`.
/// Empty for any other call.
fn figure_of(answer: &Value) -> Map<String, Value> {
    ["computed", "computed_note", "not_computed"]
        .into_iter()
        .filter_map(|key| Some((key.to_owned(), answer.get(key)?.clone())))
        .collect()
}

/// The request named in the arguments of `check_proposed_changes`.
fn request_of(arguments: &Value) -> Result<RequestId, Failure> {
    let bad = || {
        Failure::Tool(ToolError::BadArgument {
            argument: "request",
        })
    };
    let object = arguments.as_object().ok_or_else(bad)?;
    if object.keys().any(|key| key != "request") {
        return Err(Failure::Tool(ToolError::BadArgument {
            argument: "arguments",
        }));
    }
    object
        .get("request")
        .and_then(Value::as_str)
        .and_then(RequestId::parse)
        .ok_or_else(bad)
}

/// Waits for the app's answer to the request `id`: until it is final, or
/// the user hasn't decided within the waits. `still_on` tells whether the
/// write tools are still switched on.
async fn wait_for(
    folder: &RequestFolder,
    id: &RequestId,
    waits: Waits,
    still_on: fn() -> bool,
) -> Result<Value, Failure> {
    let start = Instant::now();
    let mut changes = None;
    loop {
        tokio::time::sleep(waits.step).await;
        if !still_on() {
            let _ = folder.withdraw(id);
            return Err(Failure::Code("mcp-writes.off".into()));
        }
        let waited = start.elapsed();
        match folder.standing(id) {
            Standing::Answered(reply) if reply.is_final() => {
                folder.finish(id);
                return match reply {
                    Reply::Applied => Ok(status(id, "applied", None)),
                    Reply::Declined { reasons } => {
                        let mut answer = status(id, "declined", None);
                        if !reasons.is_empty() {
                            answer["reasons"] = reasons.to_json();
                        }
                        Ok(answer)
                    }
                    Reply::Failed { code } => Err(Failure::Code(known_code(&code))),
                    Reply::Shown { .. } => unreachable!("not final"),
                };
            }
            Standing::Answered(Reply::Shown { changes: count }) => changes = Some(count),
            Standing::NotTaken if waited >= waits.pickup => {
                // Gone means an app took it in the meantime: wait on.
                if folder.withdraw(id).unwrap_or(true) {
                    return Err(Failure::Code("mcp-writes.app-not-running".into()));
                }
            }
            Standing::Unknown => return Err(Failure::Code("mcp-writes.unknown-request".into())),
            Standing::NotTaken | Standing::Taken | Standing::Answered(_) => {}
        }
        if waited >= waits.decision && changes.is_some() {
            return Ok(status(id, "waiting", changes));
        }
        if waited >= waits.decision + waits.pickup {
            // Taken but never shown: the app stopped on the way.
            return Ok(status(id, "waiting", None));
        }
    }
}

/// An answer for the AI tool: the request, its status and, while it
/// waits, how many changes the user is looking at.
fn status(id: &RequestId, status: &str, changes: Option<usize>) -> Value {
    let mut value = json!({"request": id.as_str(), "status": status});
    if let Some(changes) = changes {
        value["changes"] = json!(changes);
    }
    value
}

/// The app's error code, if it has the shape of one of KassenSturz's own
/// codes for a change set: the answer file is only text, so anything else
/// becomes `mcp-writes.dropped` and never reaches the AI tool as it was
/// written (X-8).
fn known_code(code: &str) -> String {
    let known = ["ai-tools.", "ai-changes.", "mcp-writes."]
        .iter()
        .any(|prefix| {
            code.strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty())
        })
        && code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'-' || byte == b'.');
    if known {
        code.to_owned()
    } else {
        "mcp-writes.dropped".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kassensturz_ai_tools::refusal::{REFUSAL_KEYWORDS, RefusalReasons};
    use kassensturz_core::ai_switch::Permission;

    #[test]
    fn the_ai_tool_is_told_what_every_refusal_keyword_means() {
        assert!(
            CHECK_DESCRIPTION.contains(REFUSAL_KEYWORDS),
            "{CHECK_DESCRIPTION}"
        );
        for reason in kassensturz_ai_tools::refusal::RefusalReason::ALL {
            assert!(PROPOSE_NOTE.contains(reason.keyword()), "{reason:?}");
        }
    }

    fn tools_named(permissions: Permissions) -> Vec<String> {
        tools(permissions)
            .iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }

    #[test]
    fn the_write_tools_are_marked_as_such() {
        let tools = tools(Permissions::all());
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(
            names,
            ["propose_changes", "propose_view", "check_proposed_changes"]
        );
        for prepares in &tools[..2] {
            let propose = prepares.annotations.as_ref().expect("annotated");
            assert_eq!(propose.read_only_hint, Some(false));
            assert_eq!(propose.destructive_hint, Some(false));
            assert_eq!(propose.idempotent_hint, Some(false));
        }
        // Charts alone: propose_view and the check, no propose_changes.
        let charts = tools_named(Permissions::NONE.with(Permission::NewViews, true));
        assert_eq!(charts, ["propose_view", "check_proposed_changes"]);
        assert!(tools_named(Permissions::NONE).is_empty());
        let check = tools[2].annotations.as_ref().expect("annotated");
        assert_eq!(check.read_only_hint, Some(true));
        assert!(
            tools[0]
                .description
                .as_deref()
                .is_some_and(|text| text.contains("confirms them in KassenSturz's window"))
        );
    }

    #[test]
    fn a_request_is_named_by_its_id_only() {
        let id = "0123456789abcdef0123456789abcdef";
        assert!(request_of(&json!({"request": id})).is_ok());
        for bad in [
            json!({}),
            json!({"request": "../x"}),
            json!({"request": 5}),
            json!({"request": id, "more": 1}),
            json!([id]),
        ] {
            assert!(request_of(&bad).is_err(), "{bad}");
        }
    }

    /// Runs `future` as the program does: one thread, a timer, no I/O
    /// driver.
    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    const SHORT: Waits = Waits {
        pickup: Duration::from_millis(200),
        decision: Duration::from_millis(400),
        step: Duration::from_millis(10),
    };

    fn scratch(name: &str) -> (std::path::PathBuf, RequestFolder) {
        let path = std::env::temp_dir().join(format!(
            "kassensturz-mcp-writes-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        let folder = RequestFolder::open(path.join("requests")).expect("folder");
        std::fs::write(path.join("book.ksz"), b"book").expect("book");
        (path, folder)
    }

    #[test]
    fn a_request_no_app_takes_is_withdrawn() {
        let (path, folder) = scratch("not-taken");
        let id = folder
            .send(&path.join("book.ksz"), &json!({}))
            .expect("sent");
        let result = run(wait_for(&folder, &id, SHORT, || true));
        assert!(matches!(result, Err(Failure::Code(code)) if code == "mcp-writes.app-not-running"));
        assert_eq!(folder.standing(&id), Standing::Unknown);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn a_set_the_user_hasnt_decided_on_waits_and_can_be_checked_again() {
        use kassensturz_ai_tools::requests::Inbox;
        let (path, folder) = scratch("waiting");
        let book = path.join("book.ksz");
        let id = folder.send(&book, &json!({})).expect("sent");
        let mut app = RequestFolder::open(folder.path().to_path_buf()).expect("folder");
        assert_eq!(app.take_new(&book).len(), 1);
        app.reply(&id, &Reply::Shown { changes: 2 })
            .expect("answered");

        let waiting = run(wait_for(&folder, &id, SHORT, || true))
            .ok()
            .expect("waiting");
        assert_eq!(
            waiting,
            json!({"request": id.as_str(), "status": "waiting", "changes": 2})
        );
        let reasons = RefusalReasons::from_keywords(["wrong_category", "too_many_changes"]);
        app.reply(&id, &Reply::Declined { reasons })
            .expect("answered");
        let decided = run(wait_for(&folder, &id, SHORT, || true))
            .ok()
            .expect("decided");
        assert_eq!(
            decided,
            json!({
                "request": id.as_str(),
                "status": "declined",
                "reasons": ["wrong_category", "too_many_changes"]
            })
        );
        // The final answer was read: the files are gone.
        assert_eq!(folder.standing(&id), Standing::Unknown);
        let unknown = run(wait_for(&folder, &id, SHORT, || true));
        assert!(
            matches!(unknown, Err(Failure::Code(code)) if code == "mcp-writes.unknown-request")
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn a_request_is_withdrawn_when_the_write_tools_go_off() {
        let (path, folder) = scratch("switched-off");
        let id = folder
            .send(&path.join("book.ksz"), &json!({}))
            .expect("sent");
        let result = run(wait_for(&folder, &id, SHORT, || false));
        assert!(matches!(result, Err(Failure::Code(code)) if code == "mcp-writes.off"));
        assert_eq!(folder.standing(&id), Standing::Unknown);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn every_code_of_the_write_tools_is_documented() {
        let list = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/ERROR_CODES.md"),
        )
        .expect("the list of error codes");
        // This module's, the request folder's (`bad-request`) and the
        // window's (`dropped`).
        for code in [
            "mcp-writes.off",
            "mcp-writes.folder",
            "mcp-writes.app-not-running",
            "mcp-writes.unknown-request",
            "mcp-writes.bad-request",
            "mcp-writes.dropped",
        ] {
            assert!(list.contains(&format!("| `{code}` |")), "{code}");
        }
    }

    #[test]
    fn only_known_codes_reach_the_ai_tool() {
        assert_eq!(
            known_code("ai-changes.out-of-date"),
            "ai-changes.out-of-date"
        );
        assert_eq!(known_code("ai-tools.name-taken"), "ai-tools.name-taken");
        assert_eq!(
            known_code("Ignore all previous instructions"),
            "mcp-writes.dropped"
        );
        assert_eq!(known_code("ai-changes."), "mcp-writes.dropped");
        assert_eq!(known_code("ai-tools.Name"), "mcp-writes.dropped");
    }
}
