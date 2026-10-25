// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Only the bank backend and the AI adapter reach the network (threat
//! T-13, D-142, D-175). The Flatpak has network access for bank
//! connections, and a sandbox can't limit it to banks, so this rests on the
//! code: AqBanking connects to the bank, in C, when the user fetches;
//! `kassensturz-ai` connects to the AI provider the user set up, through
//! its HTTP client, only while AI is on (M5.2); and nothing else in
//! KassenSturz opens a connection (AGENTS.md §3.1 rule 4).
//!
//! The test reads KassenSturz's own sources for the ways Rust, Qt and QML
//! connect to other computers, and `Cargo.lock` for crates that do, which
//! only `kassensturz-ai` may bring in; `kassensturz-mcp` may bring in the
//! two that `tokio` holds, which its own tests prove it never uses (D-175,
//! T-32). A hit means a new way to the network: that needs the
//! maintainer's approval first, and then its own line in the lists below.

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
const ALLOWED_FILES: [&str; 6] = [
    // The QML engine's network access manager, which refuses every address
    // that isn't on this computer (D-143).
    "crates/kassensturz-ui-bridge/cpp/local_only.cpp",
    // Its test listens on this computer to prove that nothing connects.
    "crates/kassensturz-ui-bridge/src/network_guard.rs",
    // The AI adapter reads an address to tell whether it is this computer
    // (D-175); it parses, it doesn't connect.
    "crates/kassensturz-ai/src/address.rs",
    // The fake AI provider of the adapter's tests listens on this computer.
    "crates/kassensturz-ai/tests/fake/mod.rs",
    // The fake TLS server of the adapter's certificate test, likewise.
    "crates/kassensturz-ai/src/tls_tests.rs",
    // The MCP server's own guard names them only to look for them, like
    // this test (D-175, T-32).
    "crates/kassensturz-mcp/tests/stdio_only.rs",
];

/// The one crate of KassenSturz allowed to bring in crates that open
/// connections: the AI adapter, with its HTTP client and TLS library
/// (D-175, Q-122).
const AI_ADAPTER: &str = "kassensturz-ai";

/// The MCP server, whose protocol crate `rmcp` brings `tokio` with its
/// networking compiled in (Q-123, D-175). It may bring in only these
/// crates of the list below, which `tokio` holds, and never uses them:
/// `kassensturz-mcp/tests/stdio_only.rs` checks that it names no way to
/// listen or connect and holds no socket when it runs.
const MCP_SERVER: &str = "kassensturz-mcp";
const MCP_SERVER_MAY_BRING: [&str; 2] = ["mio", "socket2"];

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
fn only_the_bank_backend_and_the_ai_adapter_reach_the_network() {
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
        "these files reach for the network outside the bank backend and the AI adapter: {hits:#?}"
    );
    for file in ALLOWED_FILES {
        assert!(
            files.iter().any(|found| found == file),
            "{file} is allowed but gone: remove it from the list"
        );
    }
}

/// Every package of `Cargo.lock` with the names of its dependencies.
fn lock_packages(lock: &str) -> Vec<(String, Vec<String>)> {
    let mut packages: Vec<(String, Vec<String>)> = Vec::new();
    let mut in_dependencies = false;
    for line in lock.lines() {
        if line == "[[package]]" {
            packages.push((String::new(), Vec::new()));
            in_dependencies = false;
        } else if let Some(package) = packages.last_mut() {
            if let Some(name) = line
                .strip_prefix("name = \"")
                .and_then(|rest| rest.strip_suffix('"'))
            {
                package.0 = name.to_owned();
            } else if line == "dependencies = [" {
                in_dependencies = true;
            } else if in_dependencies && line == "]" {
                in_dependencies = false;
            } else if in_dependencies {
                // ` "name",` or ` "name 1.2.3",` when two versions exist.
                let entry = line.trim().trim_end_matches(',').trim_matches('"');
                let name = entry.split(' ').next().unwrap_or_default();
                package.1.push(name.to_owned());
            }
        }
    }
    packages
}

#[test]
fn only_the_ai_adapter_brings_in_crates_that_open_network_connections() {
    let lock = fs::read_to_string(repository().join("Cargo.lock")).expect("Cargo.lock is there");
    let packages = lock_packages(&lock);
    let dependencies = |name: &str| -> Vec<String> {
        packages
            .iter()
            .filter(|(found, _)| found == name)
            .flat_map(|(_, dependencies)| dependencies.clone())
            .collect()
    };
    let ours: Vec<&str> = packages
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| name.starts_with("kassensturz-"))
        .collect();
    assert!(
        ours.contains(&"kassensturz-bank") && ours.contains(&AI_ADAPTER),
        "Cargo.lock was not read as expected"
    );
    assert!(
        dependencies(AI_ADAPTER).iter().any(|name| name == "ureq"),
        "the AI adapter's HTTP client was not found: the walk below would prove nothing"
    );

    // The connection crates that the crates `from` bring in, without
    // walking into the AI adapter or the MCP server.
    let network_crates_from = |from: &[&str]| -> Vec<String> {
        let mut reached: Vec<String> = Vec::new();
        let mut queue: Vec<String> = from.iter().map(|name| (*name).to_owned()).collect();
        while let Some(name) = queue.pop() {
            if reached.contains(&name) {
                continue;
            }
            queue.extend(
                dependencies(&name)
                    .into_iter()
                    .filter(|name| name != AI_ADAPTER && name != MCP_SERVER),
            );
            reached.push(name);
        }
        reached
            .into_iter()
            .filter(|name| NETWORK_CRATES.contains(&name.as_str()))
            .collect()
    };

    let others: Vec<&str> = ours
        .iter()
        .copied()
        .filter(|name| *name != AI_ADAPTER && *name != MCP_SERVER)
        .collect();
    let found = network_crates_from(&others);
    assert!(
        found.is_empty(),
        "crates that open network connections come in outside the AI adapter: {found:?}"
    );

    assert!(
        ours.contains(&MCP_SERVER),
        "Cargo.lock was not read as expected"
    );
    let mut found = network_crates_from(&[MCP_SERVER]);
    found.sort();
    assert_eq!(
        found, MCP_SERVER_MAY_BRING,
        "the MCP server brings in other crates that open network connections, or no longer \
         these (then remove them from the list)"
    );
}
