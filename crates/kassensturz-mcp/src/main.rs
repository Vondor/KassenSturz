// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `kassensturz-mcp` program: the local MCP server that lets the user's
//! own AI tools read the book over stdio (D-050, D-194).
//!
//! The user's AI tool starts this program itself and talks to it through
//! its standard input and output, the stdio transport of the Model Context
//! Protocol. It opens no port and no socket (D-175): `rmcp` is built with
//! only its server and stdio features, the runtime has no I/O driver, and
//! tests fail on a network transport, on a listening or connecting call in
//! these sources, or on a socket the running program holds (threats T-13,
//! T-32).
//!
//! It refuses to start unless AI is switched on in the app's settings
//! (D-013), and stops at the next request once it is switched off. It only
//! reads: the tools are the read functions of `kassensturz-ai-tools`, the
//! same the in-app chat uses, and the book is opened read-only beside the
//! running app, one short read per answer (D-195, Q-036). While the user has
//! turned the write tools on, it also hands prepared changes to the running
//! app as request files, and the user confirms them there (D-216, D-217).
//! A build without AI (D-039) holds nothing else than a message saying so.

#![forbid(unsafe_code)]
// Standard output carries the protocol; anything else written there would
// break it. Messages for the user go to standard error, which AI tools show
// in their logs.
#![deny(clippy::print_stdout)]

use std::process::ExitCode;

#[cfg(feature = "ai")]
mod book_path;
#[cfg(feature = "ai")]
mod server;
#[cfg(feature = "ai")]
mod writes;

/// The exit status of a refusal to start.
const REFUSED: u8 = 2;

#[cfg(feature = "ai")]
fn main() -> ExitCode {
    use kassensturz_core::Report;

    if !kassensturz_core::ai_switch::enabled() {
        eprintln!("{}", server::SWITCHED_OFF);
        return ExitCode::from(REFUSED);
    }
    if std::env::args_os()
        .nth(1)
        .is_some_and(|flag| flag == "--request-folder")
    {
        return show_request_folder();
    }
    let book = match book_path::from_arguments(std::env::args_os().skip(1)) {
        Ok(book) => book,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(REFUSED);
        }
    };
    // Checked once at the start, so a wrong book shows in the AI tool's log
    // right away; every answer opens the book again (D-195).
    if let Err(error) = kassensturz_store::Book::open_to_read(&book) {
        eprintln!(
            "error: the book {} can't be read ({}). Check the path, or open the book in \
             KassenSturz once.",
            book.display(),
            error.report().code
        );
        return ExitCode::from(REFUSED);
    }
    // One thread, a timer for the protocol's timeouts, and no I/O driver:
    // standard input and output are read and written on Tokio's blocking
    // threads, so nothing here ever polls a socket (D-194).
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: the server couldn't start: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(server::serve(book)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `--request-folder`: makes the folder through which the write tools reach
/// the running app, if needed, and names it on standard error (D-216). It
/// shows where a request goes, for example that `flatpak run
/// --command=kassensturz-mcp` and the app meet in the same folder (D-219).
#[cfg(feature = "ai")]
fn show_request_folder() -> ExitCode {
    use kassensturz_ai_tools::requests::RequestFolder;

    if !kassensturz_core::ai_switch::write_tools_enabled() {
        eprintln!(
            "error: the write tools are switched off. Turn on \"Let my AI tools prepare \
             changes\" in KassenSturz's AI settings first."
        );
        return ExitCode::from(REFUSED);
    }
    let Some(place) = RequestFolder::default_place() else {
        eprintln!("error: this system has no request folder yet.");
        return ExitCode::from(REFUSED);
    };
    match RequestFolder::open(place) {
        Ok(folder) => {
            eprintln!("request folder: {}", folder.path().display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: the request folder can't be used: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(feature = "ai"))]
fn main() -> ExitCode {
    eprintln!("error: this build of KassenSturz was made without AI features.");
    ExitCode::from(REFUSED)
}
