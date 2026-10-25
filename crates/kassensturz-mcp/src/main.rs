// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `kassensturz-mcp` program: the local MCP server that lets the user's
//! own AI tools read the book over stdio (D-050).
//!
//! Until M5.6 this is a stub that only keeps the promise of the AI switch: it
//! refuses to start unless AI is switched on in the app's settings (D-013),
//! and a build without AI (D-039) holds nothing else than a message saying
//! so.

use std::process::ExitCode;

/// The exit status of a refusal to start.
const REFUSED: u8 = 2;

#[cfg(feature = "ai")]
fn main() -> ExitCode {
    if !kassensturz_core::ai_switch::enabled() {
        eprintln!(
            "error: AI features are switched off. Switch them on in KassenSturz's settings \
             first; this program does not start while they are off."
        );
        return ExitCode::from(REFUSED);
    }
    eprintln!("error: the MCP server is not built yet (it arrives with M5.6).");
    ExitCode::from(REFUSED)
}

#[cfg(not(feature = "ai"))]
fn main() -> ExitCode {
    eprintln!("error: this build of KassenSturz was made without AI features.");
    ExitCode::from(REFUSED)
}
