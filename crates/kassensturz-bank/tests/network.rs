// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Only the bank backend reaches the network (threat T-13, D-142). The
//! Flatpak has network access for bank connections, and a sandbox can't
//! limit it to banks, so this rests on the code: AqBanking connects to the
//! bank, in C, when the user fetches, and nothing else in KassenSturz opens
//! a connection (AGENTS.md §3.1 rule 4).
//!
//! The test reads KassenSturz's own sources for the ways Rust, Qt and QML
//! connect to other computers, and `Cargo.lock` for crates that do. A hit
//! means a new way to the network: that needs the maintainer's approval
//! first, and then its own line in the lists below.

use std::fs;
use std::path::{Path, PathBuf};

/// The ways KassenSturz's Rust, C++ and QML code could connect to another
/// computer.
const NETWORK_WORDS: [&str; 14] = [
    "std::net",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "ToSocketAddrs",
    "tokio::net",
    "QtNetwork",
    "QNetworkAccessManager",
    "QTcpSocket",
    "QUdpSocket",
    "QSslSocket",
    "QWebSocket",
    "XMLHttpRequest",
    "WebSocket",
];

/// Files allowed to name them, relative to the repository, and why.
const ALLOWED_FILES: [&str; 2] = [
    // The QML engine's network access manager, which refuses every address
    // that isn't on this computer (D-143).
    "crates/kassensturz-ui-bridge/cpp/local_only.cpp",
    // Its test listens on this computer to prove that nothing connects.
    "crates/kassensturz-ui-bridge/src/network_guard.rs",
];

/// Crates that open network connections: HTTP clients and servers, TLS,
/// WebSockets, DNS and socket libraries. `zbus`, which the keychain uses
/// (D-147), is not among them: it talks to the Secret Service over the
/// session bus, a local socket.
const NETWORK_CRATES: [&str; 22] = [
    "attohttpc",
    "curl",
    "curl-sys",
    "h2",
    "h3",
    "hickory-resolver",
    "hyper",
    "hyper-util",
    "isahc",
    "mio",
    "native-tls",
    "openssl",
    "quinn",
    "reqwest",
    "rustls",
    "socket2",
    "surf",
    "tokio-tungstenite",
    "trust-dns-resolver",
    "tungstenite",
    "ureq",
    "websocket",
];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every source file of KassenSturz's crates, as a path relative to the
/// repository with `/` between folders.
fn source_files(root: &Path, folder: &Path, found: &mut Vec<String>) {
    let entries = fs::read_dir(root.join(folder)).expect("a folder of the crates can be read");
    for entry in entries {
        let entry = entry.expect("a folder entry can be read");
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = folder.join(&name);
        if entry.file_type().expect("a file type can be read").is_dir() {
            // Build output, should a build have left some here.
            if name != "target" {
                source_files(root, &path, found);
            }
        } else if [".rs", ".cpp", ".h", ".c", ".qml", ".js"]
            .iter()
            .any(|ending| name.ends_with(ending))
        {
            let parts: Vec<String> = path
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            found.push(parts.join("/"));
        }
    }
}

#[test]
fn only_the_bank_backend_reaches_the_network() {
    let root = repository();
    let this_file = "crates/kassensturz-bank/tests/network.rs";
    let mut files = Vec::new();
    source_files(&root, Path::new("crates"), &mut files);
    assert!(
        files.iter().any(|file| file == this_file),
        "the walk must find this test's own file, or it looked in the wrong place"
    );
    assert!(files.len() > 100, "only {} source files found", files.len());

    let mut hits = Vec::new();
    for file in &files {
        if file == this_file || ALLOWED_FILES.contains(&file.as_str()) {
            continue;
        }
        let text = fs::read_to_string(root.join(file)).expect("a source file is text");
        for word in NETWORK_WORDS {
            if text.contains(word) {
                hits.push(format!("{file}: {word}"));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "these files reach for the network outside the bank backend: {hits:#?}"
    );
    for file in ALLOWED_FILES {
        assert!(
            files.iter().any(|found| found == file),
            "{file} is allowed but gone: remove it from the list"
        );
    }
}

#[test]
fn no_crate_that_opens_network_connections_is_built() {
    let lock = fs::read_to_string(repository().join("Cargo.lock")).expect("Cargo.lock is there");
    let names: Vec<&str> = lock
        .lines()
        .filter_map(|line| line.strip_prefix("name = \""))
        .filter_map(|rest| rest.strip_suffix('"'))
        .collect();
    assert!(
        names.contains(&"kassensturz-bank"),
        "Cargo.lock was not read as expected"
    );
    let found: Vec<&&str> = names
        .iter()
        .filter(|name| NETWORK_CRATES.contains(name))
        .collect();
    assert!(
        found.is_empty(),
        "crates that open network connections are in Cargo.lock: {found:?}"
    );
}
