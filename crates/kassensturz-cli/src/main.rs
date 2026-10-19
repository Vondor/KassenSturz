// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `kassensturz-cli` program: reads its arguments and environment, runs
//! one command and exits with its status.

use std::process::ExitCode;

use kassensturz_cli::{Environment, run};

fn main() -> ExitCode {
    let args: Result<Vec<String>, _> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.into_string())
        .collect();
    let Ok(args) = args else {
        eprintln!("error: the arguments must be valid UTF-8 text");
        return ExitCode::from(2);
    };
    let status = run(
        &args,
        &Environment::from_system(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    ExitCode::from(status)
}
