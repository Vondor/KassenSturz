// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A made-up KeePassXC for the tests: its browser interface on a socket in
//! a test folder, written from KeePassXC's protocol description, with the
//! database in memory. It answers like KeePassXC where KassenSturz depends
//! on it: plain key exchange, encrypted messages with the nonce plus one,
//! plain error answers with a code as text, a locked database that the
//! "user" unlocks after `triggerUnlock`, and no way to delete an entry.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use crypto_box::aead::{Aead, OsRng};
use crypto_box::{Nonce, PublicKey, SalsaBox, SecretKey as BoxKey};
use serde_json::{Value, json};

use super::increment;

/// One entry of the made-up database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeEntry {
    pub uuid: String,
    pub url: String,
    pub login: String,
    pub password: String,
}

/// What the made-up KeePassXC holds and how its "user" answers.
#[derive(Debug, Default)]
pub struct FakeState {
    /// Whether the database is locked; `triggerUnlock` unlocks it.
    pub locked: bool,
    /// Whether the user allows a new connection.
    pub refuse_association: bool,
    /// The connections allowed: name and key.
    pub links: Vec<(String, String)>,
    pub entries: Vec<FakeEntry>,
    /// The actions asked for, in order, to check what was sent.
    pub actions: Vec<String>,
    /// Every request as it came over the socket, to check that no secret
    /// travelled in clear.
    pub raw: Vec<String>,
}

/// The made-up KeePassXC: serves connections one after another until the
/// test ends.
pub struct FakeKeePassXc {
    pub socket: PathBuf,
    pub state: Arc<Mutex<FakeState>>,
    folder: PathBuf,
}

impl FakeKeePassXc {
    pub fn start(name: &str) -> Self {
        let folder = std::env::temp_dir().join(format!(
            "ks-kpxc-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|time| time.subsec_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&folder).expect("test folder");
        let socket = folder.join("browser.socket");
        let listener = UnixListener::bind(&socket).expect("socket");
        let state = Arc::new(Mutex::new(FakeState::default()));
        let shared = Arc::clone(&state);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                serve(stream, &shared);
            }
        });
        Self {
            socket,
            state,
            folder,
        }
    }

    pub fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state.lock().expect("fake state")
    }
}

impl Drop for FakeKeePassXc {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn serve(mut stream: UnixStream, state: &Mutex<FakeState>) {
    let own = BoxKey::generate(&mut OsRng);
    let mut client: Option<SalsaBox> = None;
    let mut received = Vec::new();
    loop {
        let mut buffer = [0_u8; 4096];
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => received.extend_from_slice(&buffer[..read]),
        }
        loop {
            let mut values = serde_json::Deserializer::from_slice(&received).into_iter::<Value>();
            let Some(Ok(request)) = values.next() else {
                break;
            };
            let end = values.byte_offset();
            state
                .lock()
                .expect("fake state")
                .raw
                .push(String::from_utf8_lossy(&received[..end]).into_owned());
            received.drain(..end);
            let replies = answer(&request, &own, &mut client, state);
            for reply in replies {
                if stream.write_all(reply.to_string().as_bytes()).is_err() {
                    return;
                }
            }
        }
    }
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn nonce_of(request: &Value) -> [u8; 24] {
    let bytes = BASE64.decode(field(request, "nonce")).expect("nonce");
    bytes.try_into().expect("24 bytes")
}

fn error(action: &str, code: u32) -> Value {
    json!({ "action": action, "errorCode": code.to_string(), "error": "made up" })
}

fn answer(
    request: &Value,
    own: &BoxKey,
    client: &mut Option<SalsaBox>,
    state: &Mutex<FakeState>,
) -> Vec<Value> {
    let action = field(request, "action").to_owned();
    let nonce = nonce_of(request);
    if action == "change-public-keys" {
        let key = BASE64.decode(field(request, "publicKey")).expect("key");
        let key = PublicKey::from_slice(&key).expect("key");
        *client = Some(SalsaBox::new(&key, own));
        state
            .lock()
            .expect("fake state")
            .actions
            .push(action.clone());
        return vec![json!({
            "action": action,
            "version": "2.7.10",
            "publicKey": BASE64.encode(own.public_key().as_bytes()),
            "nonce": BASE64.encode(increment(nonce)),
            "success": "true",
        })];
    }
    let Some(sealed_box) = client.as_ref() else {
        return vec![error(&action, 3)];
    };
    let sealed = BASE64.decode(field(request, "message")).expect("message");
    let Ok(plain) = sealed_box.decrypt(Nonce::from_slice(&nonce), sealed.as_slice()) else {
        return vec![error(&action, 4)];
    };
    let message: Value = serde_json::from_slice(&plain).expect("JSON");
    let mut state = state.lock().expect("fake state");
    state.actions.push(action.clone());
    if state.locked {
        if action == "get-databasehash" && field(request, "triggerUnlock") == "true" {
            // The user unlocks the database in KeePassXC's window.
            state.locked = false;
            return vec![
                error(&action, 1),
                json!({ "action": "database-locked" }),
                json!({ "action": "database-unlocked" }),
            ];
        }
        return vec![error(&action, 1)];
    }
    let known = |id: &str, key: &str| {
        state
            .links
            .iter()
            .any(|(known_id, known_key)| known_id == id && known_key == key)
    };
    let reply = match action.as_str() {
        "get-databasehash" => {
            json!({ "action": "hash", "hash": "29234e32274a32276e25666a42", "version": "2.7.10" })
        }
        "associate" => {
            if state.refuse_association {
                return vec![error(&action, 6)];
            }
            let id = format!("KassenSturz {}", state.links.len() + 1);
            state
                .links
                .push((id.clone(), field(&message, "idKey").to_owned()));
            json!({ "id": id, "hash": "29234e32274a32276e25666a42", "success": "true" })
        }
        "test-associate" => {
            if !known(field(&message, "id"), field(&message, "key")) {
                return vec![error(&action, 8)];
            }
            json!({ "id": field(&message, "id"), "success": "true" })
        }
        "get-logins" => {
            let keys = message.get("keys").and_then(Value::as_array);
            let allowed = keys.is_some_and(|keys| {
                keys.iter()
                    .any(|key| known(field(key, "id"), field(key, "key")))
            });
            if !allowed {
                return vec![error(&action, 6)];
            }
            let url = field(&message, "url");
            let entries: Vec<Value> = state
                .entries
                .iter()
                .filter(|entry| entry.url == url)
                .map(|entry| {
                    json!({
                        "login": entry.login,
                        "name": url,
                        "password": entry.password,
                        "uuid": entry.uuid,
                        "group": "KeePassXC-Browser Passwords",
                    })
                })
                .collect();
            if entries.is_empty() {
                return vec![error(&action, 15)];
            }
            json!({ "count": entries.len().to_string(), "entries": entries, "success": "true" })
        }
        "set-login" => {
            let uuid = field(&message, "uuid").to_owned();
            if uuid.is_empty() {
                let uuid = format!("{:032x}", state.entries.len() + 1);
                let entry = FakeEntry {
                    uuid,
                    url: field(&message, "url").to_owned(),
                    login: field(&message, "login").to_owned(),
                    password: field(&message, "password").to_owned(),
                };
                state.entries.push(entry);
            } else if let Some(entry) = state.entries.iter_mut().find(|entry| entry.uuid == uuid) {
                entry.password = field(&message, "password").to_owned();
            }
            json!({ "count": null, "entries": null, "error": "", "success": "true" })
        }
        _ => return vec![error(&action, 12)],
    };
    let reply_nonce = increment(nonce);
    let sealed = sealed_box
        .encrypt(
            Nonce::from_slice(&reply_nonce),
            reply.to_string().as_bytes(),
        )
        .expect("encrypt");
    vec![json!({
        "action": action,
        "message": BASE64.encode(sealed),
        "nonce": BASE64.encode(reply_nonce),
    })]
}
