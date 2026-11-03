// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Starts KassenSturz again, for the setup wizard's language choice (M7a.9,
//! Q-395): on its first page nothing has been typed yet, so a restart loses
//! nothing, and the whole app comes up in the new language.
//!
//! The new KassenSturz gets the environment this one was started with, as
//! `main` found it before it changed anything: the language (D-127) and the
//! test profile (Q-399) set variables at the start, and the new process must
//! work them out again from the settings, not inherit this one's results. In
//! the Flatpak a process started inside the sandbox ends with it, so there
//! the new KassenSturz is started in a sandbox of its own through the
//! Flatpak portal (`flatpak-spawn`), which needs no extra permission; the
//! KassenSturz variables it was started with (`KASSENSTURZ_*`) go along.

use std::ffi::OsString;
use std::sync::OnceLock;

/// The environment and arguments the app was started with.
struct Start {
    environment: Vec<(OsString, OsString)>,
    arguments: Vec<OsString>,
}

static START: OnceLock<Start> = OnceLock::new();

/// Remembers how the app was started. The first statement of `main`, before
/// anything changes the environment.
pub fn remember_at_start() {
    let _ = START.set(Start {
        environment: std::env::vars_os().collect(),
        arguments: std::env::args_os().skip(1).collect(),
    });
}

/// What to start, and with which environment (`None`: the portal gives the
/// new sandbox its own, plus what the arguments name).
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub program: String,
    pub arguments: Vec<String>,
    pub environment: Option<Vec<String>>,
}

/// The command name of the app inside the Flatpak (`command` in its manifest).
const FLATPAK_COMMAND: &str = "kassensturz";

/// The prefix of KassenSturz's own variables, which go along into a new
/// Flatpak sandbox.
const OWN_VARIABLES: &str = "KASSENSTURZ_";

/// How to start the app `program` again with `arguments` and `environment`,
/// in the Flatpak (`in_flatpak`) or not.
pub fn plan(
    in_flatpak: bool,
    program: &str,
    arguments: &[OsString],
    environment: &[(OsString, OsString)],
) -> Plan {
    let text = |value: &OsString| value.to_string_lossy().into_owned();
    let arguments: Vec<String> = arguments.iter().map(text).collect();
    if in_flatpak {
        let mut spawn: Vec<String> = environment
            .iter()
            .filter(|(name, _)| name.to_string_lossy().starts_with(OWN_VARIABLES))
            .map(|(name, value)| format!("--env={}={}", text(name), text(value)))
            .collect();
        spawn.push(FLATPAK_COMMAND.to_owned());
        spawn.extend(arguments);
        return Plan {
            program: "flatpak-spawn".to_owned(),
            arguments: spawn,
            environment: None,
        };
    }
    Plan {
        program: program.to_owned(),
        arguments,
        environment: Some(
            environment
                .iter()
                .map(|(name, value)| format!("{}={}", text(name), text(value)))
                .collect(),
        ),
    }
}

/// Starts the app again and ends this one. Returns false, and changes
/// nothing, when the new one couldn't be started.
pub fn restart() -> bool {
    let Some(start) = START.get() else {
        return false;
    };
    let plan = plan(
        crate::platform::in_flatpak_sandbox(),
        &crate::platform::app_file_path(),
        &start.arguments,
        &start.environment,
    );
    if !crate::platform::start_detached(&plan) {
        return false;
    }
    // Closed now, not when the event loop has ended, so the new
    // KassenSturz finds the book free (D-038).
    crate::session::close();
    crate::platform::quit();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(value: &str) -> OsString {
        OsString::from(value)
    }

    fn environment() -> Vec<(OsString, OsString)> {
        vec![
            (os("PATH"), os("/usr/bin")),
            (os("KASSENSTURZ_TEST"), os("1")),
            (os("LANG"), os("de_DE.UTF-8")),
        ]
    }

    #[test]
    fn outside_the_flatpak_the_same_program_starts_with_the_environment_it_was_started_with() {
        let plan = plan(
            false,
            "/usr/bin/kassensturz",
            &[os("--some-argument")],
            &environment(),
        );
        assert_eq!(
            plan,
            Plan {
                program: "/usr/bin/kassensturz".to_owned(),
                arguments: vec!["--some-argument".to_owned()],
                environment: Some(vec![
                    "PATH=/usr/bin".to_owned(),
                    "KASSENSTURZ_TEST=1".to_owned(),
                    "LANG=de_DE.UTF-8".to_owned(),
                ]),
            }
        );
    }

    #[test]
    fn in_the_flatpak_a_new_sandbox_starts_with_kassensturzs_own_variables() {
        let plan = plan(true, "/app/bin/kassensturz", &[], &environment());
        assert_eq!(
            plan,
            Plan {
                program: "flatpak-spawn".to_owned(),
                arguments: vec![
                    "--env=KASSENSTURZ_TEST=1".to_owned(),
                    "kassensturz".to_owned()
                ],
                environment: None,
            }
        );
    }
}
