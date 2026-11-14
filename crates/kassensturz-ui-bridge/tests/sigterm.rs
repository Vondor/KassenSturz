// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The app ends cleanly on SIGTERM (B-276): a logout, a shutdown or `pkill`
//! must end the event loop like quitting does, so `main` closes the book
//! and removes its lock file. Without a handler the signal kills the app on
//! the spot and the lock file stays behind.
//!
//! The app is started in the test profile (`KASSENSTURZ_TEST=1`, every
//! folder in a test folder) under Qt's `offscreen` platform. If it ends on
//! its own before the signal, the test fails and shows what the app said:
//! it never passes without having sent the signal to a running app.

#![cfg(unix)]

use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn stop(signal: &str) {
    let app = env!("CARGO_BIN_EXE_kassensturz");
    let home = std::env::temp_dir().join(format!(
        "kassensturz-sigterm-{}-{signal}",
        std::process::id()
    ));
    std::fs::create_dir_all(&home).expect("a temporary folder");
    let mut child = Command::new(app)
        .env("KASSENSTURZ_TEST", "1")
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("HOME", &home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the app starts");
    // Time to load the screens and install the handler.
    std::thread::sleep(Duration::from_secs(5));
    if let Some(status) = child.try_wait().expect("the app can be asked") {
        let mut said = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut said);
        }
        panic!("the app ended by itself before the signal ({status}): {said}");
    }
    let sent = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(child.id().to_string())
        .status()
        .expect("kill runs");
    assert!(sent.success(), "the signal was sent");
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("the app can be asked") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("the app did not end within 20 seconds of {signal}");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(
        status.signal(),
        None,
        "{signal} must end the app through its own way out, not kill it"
    );
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        status.code(),
        Some(0),
        "{signal} ends the app like quitting"
    );
}

#[test]
fn sigterm_ends_the_app_through_its_own_way_out() {
    stop("TERM");
}

#[test]
fn sighup_ends_the_app_through_its_own_way_out() {
    stop("HUP");
}
