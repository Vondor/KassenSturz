// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The server speaks stdio only and opens no port or socket (D-050, D-175;
//! threats T-13, T-32). `rmcp` brings `tokio` with its networking compiled
//! in, so this rests on three tests instead of on the absence of a
//! connection crate:
//!
//! - `rmcp` is built with only its server and stdio transport features: the
//!   workspace asks for nothing else, no other crate asks for it, and
//!   `Cargo.lock` holds exactly the dependencies those features bring, so a
//!   network transport (HTTP, SSE, WebSockets, a Unix socket client) can't
//!   come in unnoticed;
//! - no source of this crate names a way to listen or connect;
//! - the running program holds no socket, and no I/O driver that could
//!   wait on one.
//!
//! A hit means a new way to the network: that needs the maintainer's
//! approval first (AGENTS.md §3.1 rule 4), then a decision that changes
//! D-050.

use std::fs;
use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The workspace's `rmcp` line, as approved (Q-123, D-175).
const RMCP: &str = r#"rmcp = { version = "=3.7.0", default-features = false, features = ["server", "transport-io"] }"#;

/// What `rmcp` depends on with those features, as `Cargo.lock` lists it. A
/// change here means `rmcp` was built differently: check which feature
/// brought it before adding it.
const RMCP_DEPENDENCIES: [&str; 14] = [
    "chrono",
    "futures",
    "indexmap",
    "libc",
    "pastey",
    "pin-project-lite",
    "schemars",
    "serde",
    "serde_json",
    "thiserror",
    "tokio",
    "tokio-util",
    "tracing",
    "uuid",
];

#[test]
fn rmcp_has_only_its_server_and_stdio_features() {
    let workspace = read(&repository().join("Cargo.toml"));
    let lines: Vec<&str> = workspace
        .lines()
        .filter(|line| line.starts_with("rmcp"))
        .collect();
    assert_eq!(lines, [RMCP], "the workspace's rmcp line changed");

    // Only this crate uses it, and adds no feature of its own.
    let crates = repository().join("crates");
    for entry in fs::read_dir(&crates).expect("crates folder") {
        let manifest = entry.expect("entry").path().join("Cargo.toml");
        if !manifest.exists() {
            continue;
        }
        let text = read(&manifest);
        let uses: Vec<&str> = text
            .lines()
            .filter(|line| line.trim_start().starts_with("rmcp"))
            .collect();
        if manifest.starts_with(env!("CARGO_MANIFEST_DIR")) {
            assert_eq!(uses, ["rmcp = { workspace = true, optional = true }"]);
        } else {
            assert!(uses.is_empty(), "{} uses rmcp", manifest.display());
        }
        assert!(
            !text.contains("rmcp/"),
            "{} switches on a feature of rmcp",
            manifest.display()
        );
    }

    let lock = read(&repository().join("Cargo.lock"));
    let entry = lock
        .split("[[package]]")
        .find(|package| package.contains("\nname = \"rmcp\"\n"))
        .expect("rmcp is in Cargo.lock");
    let dependencies: Vec<&str> = entry
        .lines()
        .skip_while(|line| *line != "dependencies = [")
        .skip(1)
        .take_while(|line| *line != "]")
        .map(|line| {
            let name = line.trim().trim_end_matches(',').trim_matches('"');
            name.split(' ').next().unwrap_or_default()
        })
        .collect();
    assert_eq!(dependencies, RMCP_DEPENDENCIES);
}

/// Ways to listen, connect or wait on a socket, in Rust and in `rmcp`.
const NETWORK_WORDS: [&str; 21] = [
    "std::net",
    "std::os::unix::net",
    "tokio::net",
    "TcpListener",
    "TcpStream",
    "TcpSocket",
    "UdpSocket",
    "UnixListener",
    "UnixStream",
    "UnixDatagram",
    "ToSocketAddrs",
    "socket2",
    "mio::",
    ".bind(",
    ".listen(",
    ".connect(",
    "enable_io",
    "enable_all",
    "streamable_http",
    "StreamableHttp",
    "stdio_pipes",
];

fn rust_files(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(folder).expect("folder read") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn no_source_names_a_way_to_listen_or_connect() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("build.rs")];
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("tests"), &mut files);
    let this_file = root.join("tests/stdio_only.rs");
    assert!(files.contains(&this_file), "the sources were not found");
    let hits: Vec<String> = files
        .iter()
        .filter(|file| **file != this_file)
        .flat_map(|file| {
            let text = read(file);
            NETWORK_WORDS
                .iter()
                .filter(|word| text.contains(*word))
                .map(|word| format!("{}: {word}", file.display()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(hits.is_empty(), "a way to the network: {hits:?}");
}

// Only Linux knows where the settings file is yet (D-177), and only Linux
// shows a program's open files in /proc.
#[cfg(all(feature = "ai", target_os = "linux"))]
mod common;

#[cfg(all(feature = "ai", target_os = "linux"))]
mod running {
    use super::common::{Folder, Session};
    use serde_json::json;

    /// What each open file of the process `pid` is, as the system names it.
    fn open_files(pid: u32) -> Vec<String> {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .expect("the program's open files")
            .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
            .map(|target| target.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_running_server_holds_no_socket() {
        let folder = Folder::new("no-socket");
        folder.switch_ai(true);
        let book = folder.book();
        let mut session = Session::start(folder.path(), &book);
        session.initialize("2025-06-18");
        session.call("list_accounts", json!({}));
        session.call("search_transactions", json!({"text": "corner"}));

        // Still running, after it answered, with the book read.
        let files = open_files(session.id());
        assert!(
            files.iter().any(|file| file.starts_with("pipe:")),
            "the files were not read: {files:?}"
        );
        let sockets: Vec<&String> = files
            .iter()
            .filter(|file| file.starts_with("socket:"))
            .collect();
        assert!(sockets.is_empty(), "the server holds a socket: {files:?}");
        // No I/O driver: nothing waits on sockets or on file descriptors.
        assert!(
            !files.iter().any(|file| file.contains("eventpoll")),
            "the server runs an I/O driver: {files:?}"
        );
        // The book is closed between answers (Q-036).
        assert!(
            !files.iter().any(|file| file.ends_with(".ksz")),
            "the book stayed open: {files:?}"
        );
        assert!(session.finish().status.success());
    }
}
