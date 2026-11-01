// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A test that reads the QML for the debug log's action events (M6c.D,
//! D-419): every handler of a user's action says that it ran, with a fixed
//! name, so a click that "does nothing" (M6-B9, B13, B14) can be told from a
//! handler that fails. QML isn't run by the unit tests, so this keeps a new
//! button from going without, the way `plain_text_audit` does for labels.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    use kassensturz_core::vocabulary::ACTIONS;

    /// The signals of a user's action: a click, a menu entry, a switch, a
    /// choice, a confirmation.
    const HANDLERS: &[&str] = &[
        "onClicked",
        "onTriggered",
        "onAccepted",
        "onToggled",
        "onActivated",
        "onChosen",
    ];

    /// Handlers that don't log, with the reason. Each is `file:handler:
    /// first line of the body` as the audit reads it. There are none: a
    /// handler that is not a user's action is not one of [`HANDLERS`].
    const EXCEPTIONS: &[&str] = &[];

    fn qml_files() -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> =
            fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("qml"))
                .expect("the qml folder")
                .map(|entry| entry.expect("a file").path())
                .filter(|path| path.extension().is_some_and(|e| e == "qml"))
                .collect();
        files.sort();
        files
    }

    /// The handler at the start of `line` (`onClicked: ...`), if it is one
    /// of [`HANDLERS`], and what follows the colon.
    fn handler(line: &str) -> Option<(&'static str, &str)> {
        let line = line.trim_start();
        HANDLERS.iter().find_map(|name| {
            let rest = line.strip_prefix(name)?.trim_start().strip_prefix(':')?;
            Some((*name, rest.trim()))
        })
    }

    /// The name in `ActionLog.taken("...")` at the start of `text`, if any.
    fn taken(text: &str) -> Option<&str> {
        let rest = text.trim_start().strip_prefix("ActionLog.taken(\"")?;
        rest.split_once("\")").map(|(name, _)| name)
    }

    /// Every handler of an action is written as `onClicked: {` (or with the
    /// arrow of a handler that takes a value) and says so on its first line.
    #[test]
    fn every_handler_of_an_action_names_an_action() {
        let mut named = BTreeSet::new();
        let mut handlers = 0;
        for path in qml_files() {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let source = fs::read_to_string(&path).expect("a readable file");
            // Handlers written inside a line, like `Button { onClicked: x }`,
            // would escape the line-by-line reading below.
            let mut mentioned = 0;
            for line in source.lines() {
                for name in HANDLERS {
                    // `onTransactionClicked:` is another signal, not `onClicked:`.
                    mentioned += line
                        .match_indices(&format!("{name}:"))
                        .filter(|(at, _)| {
                            !line[..*at].ends_with(|c: char| c.is_ascii_alphanumeric())
                        })
                        .count();
                }
            }
            let mut found = 0;
            let lines: Vec<&str> = source.lines().collect();
            for (number, line) in lines.iter().enumerate() {
                let Some((name, body)) = handler(line) else {
                    continue;
                };
                found += 1;
                handlers += 1;
                let opens_block = body.ends_with('{');
                let first = lines.get(number + 1).copied().unwrap_or_default();
                let exception = format!("{file}:{name}:{}", first.trim());
                if EXCEPTIONS.contains(&exception.as_str()) {
                    continue;
                }
                assert!(
                    opens_block,
                    "{file}:{}: `{name}: {body}` must be a block that starts with ActionLog.taken(\"...\") (M6c.D)",
                    number + 1
                );
                let Some(action) = taken(first) else {
                    panic!(
                        "{file}:{}: `{name}` must start with ActionLog.taken(\"{}_...\"), the fixed name of what it does (M6c.D)",
                        number + 2,
                        file.trim_end_matches(".qml")
                    );
                };
                assert!(
                    ACTIONS.contains(&action),
                    "{file}:{}: add {action} to kassensturz_core::vocabulary::ACTIONS",
                    number + 2
                );
                assert!(
                    action.starts_with(&format!("{}_", file.trim_end_matches(".qml"))),
                    "{file}:{}: {action} must start with the file's name",
                    number + 2
                );
                assert!(
                    named.insert(action.to_owned()),
                    "{file}:{}: {action} names two handlers; give each its own name",
                    number + 2
                );
            }
            assert_eq!(
                found, mentioned,
                "{file}: a handler of an action stands inside a line; give it a line of its own"
            );
        }
        assert!(handlers > 300, "only {handlers} handlers were found");
        let unused: Vec<_> = ACTIONS
            .iter()
            .filter(|action| !named.contains(**action))
            .collect();
        assert!(
            unused.is_empty(),
            "no handler uses these names (remove them from ACTIONS): {unused:?}"
        );
    }

    /// Every call that names an action is in a handler of an action or in a
    /// function a handler calls, never a name nobody can find again.
    #[test]
    fn every_call_that_names_an_action_names_one_of_the_list() {
        for path in qml_files() {
            let source = fs::read_to_string(&path).expect("a readable file");
            for (number, line) in source.lines().enumerate() {
                let Some(position) = line.find("ActionLog.taken(") else {
                    continue;
                };
                let Some(action) = taken(&line[position..]) else {
                    panic!(
                        "{}:{}: ActionLog.taken takes a plain string, the fixed name",
                        path.display(),
                        number + 1
                    );
                };
                assert!(ACTIONS.contains(&action), "{action}");
            }
        }
    }
}
