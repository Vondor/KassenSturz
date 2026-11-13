// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Attack scenario A6 of the outside security review: the chat transcript
//! of the developer mode (`chat.jsonl`, D-235) stays on disk. It holds the
//! book's data that was sent and what the user typed. All data is made up
//! (AGENTS.md §3.1 rule 2).

#![cfg(all(feature = "debug-log", unix))]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use kassensturz_core::debug_log::DebugLog;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "kassensturz-attack-a6-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("made");
        std::fs::write(path.join("kassensturzrc"), "[AI]\nEnabled=true\n").expect("written");
        Self(path)
    }

    fn log(&self, on: bool) -> DebugLog {
        DebugLog::at(self.0.join("debug-log"), on, self.0.join("kassensturzrc"))
    }

    fn transcript(&self) -> PathBuf {
        self.0.join("debug-log").join("chat.jsonl")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn a6_the_transcript_is_for_the_user_only() {
    let scratch = Scratch::new("mode");
    scratch
        .log(true)
        .transcript("asked", &[("question", "What is my balance?")]);
    assert_eq!(mode(&scratch.0.join("debug-log")), 0o700);
    assert_eq!(mode(&scratch.transcript()), 0o600);
}

#[test]
#[ignore = "B-253 (attack A6): the transcript stays after the developer mode is off"]
fn a6_the_transcript_is_gone_once_the_developer_mode_is_off() {
    let scratch = Scratch::new("gone");
    scratch
        .log(true)
        .transcript("asked", &[("question", "What is my balance?")]);
    assert!(scratch.transcript().exists());
    // The next start without the developer mode.
    let off = scratch.log(false);
    off.record(kassensturz_core::debug_log::Event::AppStarted, None, None);
    assert!(
        !scratch.transcript().exists(),
        "chat.jsonl with the user's questions is still there"
    );
}
