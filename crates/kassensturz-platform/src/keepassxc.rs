// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved secrets in a KeePassXC database, through KeePassXC's browser
//! interface (D-164).
//!
//! KeePassXC opens a local socket for its browser extension when the user
//! turns on "Browser Integration"; other programs may use it too, written
//! here from KeePassXC's published protocol description, not from its
//! code. Every message after the first is encrypted with NaCl's
//! `crypto_box` (`crypto_box` crate: X25519 and XSalsa20-Poly1305, the
//! format of libsodium's `crypto_box_easy`), with a key pair made for each
//! connection. KassenSturz never sees the database's password: KeePassXC
//! asks the user to unlock it, to allow KassenSturz once ("associate", the
//! user names the connection), and, if set so, to allow each read.
//!
//! A secret is one entry: its URL is `kassensturz://<service>`, the
//! service in lower case (e.g. `kassensturz://io.github.vondor.kassensturz`,
//! which is also the entry's title), its user name is the
//! [`SecretKey`] (`bank-pin/<login ID>`), its password the secret. New
//! entries go into KeePassXC's own group for them ("KeePassXC-Browser
//! Passwords"), in the database that is open in front.
//!
//! The interface can't delete an entry. Deleting a secret empties the
//! entry's password, which KeePassXC asks the user to allow; an entry with
//! an empty password counts as no secret, and the user removes the entry in
//! KeePassXC.
//!
//! The connection's key that KeePassXC accepts after "associate" is, with
//! its name, all a program needs to ask KeePassXC for entries, so it is a
//! secret itself: it is kept in the system keychain, one per database
//! (`keepassxc-link/<database hash>`), or only until the app closes when
//! there is no system keychain; KeePassXC then asks to allow KassenSturz
//! again at the next start.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use crypto_box::aead::rand_core::RngCore;
use crypto_box::aead::{Aead, AeadCore, OsRng};
use crypto_box::{Nonce, PublicKey, SalsaBox, SecretKey as BoxKey};
use serde_json::{Map, Value, json};
use zeroize::{Zeroize, Zeroizing};

use crate::{Keychain, KeychainError, Secret, SecretKey};

/// The name of KeePassXC's socket for its browser interface.
const SOCKET_NAME: &str = "org.keepassxc.KeePassXC.BrowserServer";

/// The folder KeePassXC puts the socket in under `XDG_RUNTIME_DIR`, the
/// same for KeePassXC installed natively and as a Flatpak; the Flatpak of
/// KassenSturz may reach only this folder (`--filesystem=xdg-run/app/...`).
const SOCKET_FOLDER: &str = "app/org.keepassxc.KeePassXC";

/// The kind part of the key a database's connection key is kept under in
/// the system keychain.
const LINK_KIND: &str = "keepassxc-link";

/// How long KassenSturz waits for KeePassXC's answer: long enough for the
/// user to unlock the database or answer KeePassXC's question.
const WAIT: Duration = Duration::from_secs(90);

/// KeePassXC's error codes that KassenSturz tells apart.
const DATABASE_NOT_OPENED: &str = "1";
const NO_LOGINS_FOUND: &str = "15";

/// The most bytes one answer of KeePassXC may have; more is not an answer
/// to anything KassenSturz asks.
const MAX_ANSWER: usize = 1 << 20;

/// Saved secrets in a KeePassXC database, filed under one service name.
///
/// It connects when first used and keeps the connection; a lost
/// connection, e.g. after KeePassXC restarted, is opened again once. Any
/// answer it can't use (KeePassXC not running, browser integration off,
/// the database locked and not unlocked in time, the user said no) is
/// [`KeychainError::KeePassXcUnavailable`]; nothing is then saved
/// anywhere else (AGENTS.md §3.1 rule 7).
pub struct KeePassXcKeychain {
    /// The host part of the entries' URL: the service in lower case.
    host: String,
    socket: Option<PathBuf>,
    /// Where the connection keys are kept: the system keychain.
    links: Box<dyn Keychain>,
    wait: Duration,
    session: Mutex<Option<Session>>,
}

impl std::fmt::Debug for KeePassXcKeychain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeePassXcKeychain")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

impl KeePassXcKeychain {
    /// Secrets filed under `service` in the database open in KeePassXC,
    /// with the connection keys in `links`, normally the
    /// [`crate::SystemKeychain`].
    pub fn new(service: &str, links: Box<dyn Keychain>) -> Self {
        Self::at(service, socket_path(), links)
    }

    /// The same at the socket `socket`, e.g. a fake KeePassXC in a test.
    pub fn at(service: &str, socket: Option<PathBuf>, links: Box<dyn Keychain>) -> Self {
        Self {
            host: service.to_ascii_lowercase(),
            socket,
            links,
            wait: WAIT,
            session: Mutex::new(None),
        }
    }

    /// Waits at most `wait` for each answer instead; for tests.
    pub fn waiting(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self
    }

    fn url(&self) -> String {
        format!("kassensturz://{}", self.host)
    }

    /// Runs `job` on the open, unlocked and allowed connection; when the
    /// connection was lost, once more on a new one.
    fn with_session<T>(
        &self,
        job: impl Fn(&mut Session, &Link) -> Result<T, Problem>,
    ) -> Result<T, KeychainError> {
        let mut slot = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        for attempt in 0..2 {
            if slot.is_none() {
                let socket = self
                    .socket
                    .as_deref()
                    .ok_or(KeychainError::KeePassXcUnavailable)?;
                *slot = Some(Session::open(socket, self.wait).map_err(Problem::into_error)?);
            }
            let session = slot.as_mut().expect("opened above");
            let result = self.link(session).and_then(|link| job(session, &link));
            match result {
                Err(Problem::Lost) if attempt == 0 => *slot = None,
                Err(problem) => {
                    if problem == Problem::Lost {
                        *slot = None;
                    }
                    return Err(problem.into_error());
                }
                Ok(value) => return Ok(value),
            }
        }
        Err(KeychainError::KeePassXcUnavailable)
    }

    /// The connection key for the database open in front, after asking
    /// KeePassXC to unlock it if needed: the one kept for it if KeePassXC
    /// still knows it, else a new one the user allows in KeePassXC.
    fn link(&self, session: &mut Session) -> Result<Link, Problem> {
        let hash = session.database_hash()?;
        if let Some((_, link)) = session.links.iter().find(|(known, _)| *known == hash) {
            return Ok(link.clone());
        }
        let key = SecretKey::new(LINK_KIND, &hash).map_err(|_| Problem::Refused)?;
        if let Ok(Some(kept)) = self.links.read(&key)
            && let Some(link) = Link::parse(kept.expose())
            && session.test_link(&link)?
        {
            session.links.push((hash, link.clone()));
            return Ok(link);
        }
        let link = session.associate()?;
        // Without a system keychain the key lives until the app closes.
        let _ = self.links.store(&key, &Secret::new(link.to_text()));
        session.links.push((hash, link.clone()));
        Ok(link)
    }

    /// KassenSturz's entry for `key`: its UUID and whether its password is
    /// set.
    fn find(
        &self,
        session: &mut Session,
        link: &Link,
        key: &SecretKey,
    ) -> Result<Option<Entry>, Problem> {
        let login = key.to_string();
        Ok(session
            .logins(&self.url(), link)?
            .into_iter()
            .find(|entry| entry.login == login))
    }
}

impl Keychain for KeePassXcKeychain {
    fn store(&self, key: &SecretKey, secret: &Secret) -> Result<(), KeychainError> {
        self.with_session(|session, link| {
            let uuid = self.find(session, link, key)?.map(|entry| entry.uuid);
            session.set_login(
                &self.url(),
                link,
                &key.to_string(),
                secret.expose(),
                uuid.as_deref(),
            )
        })
    }

    fn read(&self, key: &SecretKey) -> Result<Option<Secret>, KeychainError> {
        self.with_session(|session, link| {
            Ok(self
                .find(session, link, key)?
                .and_then(|entry| entry.password)
                .map(|password| Secret::new(password.to_string())))
        })
    }

    fn delete(&self, key: &SecretKey) -> Result<(), KeychainError> {
        self.with_session(|session, link| match self.find(session, link, key)? {
            Some(Entry {
                uuid,
                password: Some(_),
                ..
            }) => session.set_login(&self.url(), link, &key.to_string(), "", Some(&uuid)),
            _ => Ok(()),
        })
    }
}

/// Where KeePassXC's socket is on this computer, if it can be named.
fn socket_path() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty())?;
    Some(Path::new(&runtime).join(SOCKET_FOLDER).join(SOCKET_NAME))
}

/// Why a call to KeePassXC didn't work, inside this module.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Problem {
    /// The connection broke or KeePassXC stopped answering in time.
    Lost,
    /// KeePassXC answered with an error, or the user said no.
    Refused,
    /// KeePassXC's answer couldn't be read: not JSON, not decrypted, the
    /// wrong nonce.
    Garbled,
}

impl Problem {
    fn into_error(self) -> KeychainError {
        match self {
            Self::Lost | Self::Refused => KeychainError::KeePassXcUnavailable,
            Self::Garbled => KeychainError::Failed {
                kind: "KeePassXcAnswer",
            },
        }
    }
}

impl From<std::io::Error> for Problem {
    fn from(_: std::io::Error) -> Self {
        Self::Lost
    }
}

/// The name KeePassXC knows KassenSturz by in one database, and the
/// public key that proves it.
#[derive(Clone)]
struct Link {
    id: String,
    key: String,
}

impl Link {
    fn to_text(&self) -> String {
        json!({ "id": self.id, "key": self.key }).to_string()
    }

    fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        Some(Self {
            id: value.get("id")?.as_str()?.to_owned(),
            key: value.get("key")?.as_str()?.to_owned(),
        })
    }
}

/// One entry KeePassXC returned for KassenSturz's URL.
struct Entry {
    uuid: String,
    login: String,
    /// The password, unless it is empty.
    password: Option<Zeroizing<String>>,
}

/// One connection to KeePassXC with its own key pair.
struct Session {
    stream: UnixStream,
    received: Vec<u8>,
    own: BoxKey,
    keepassxc: SalsaBox,
    client_id: String,
    wait: Duration,
    /// The connection keys checked in this connection, by database hash.
    links: Vec<(String, Link)>,
}

impl Session {
    /// Connects and exchanges public keys.
    fn open(socket: &Path, wait: Duration) -> Result<Self, Problem> {
        let mut stream = UnixStream::connect(socket).map_err(|_| Problem::Refused)?;
        stream.set_write_timeout(Some(wait))?;
        let own = BoxKey::generate(&mut OsRng);
        let client_id = random_text();
        let nonce = random_nonce();
        let request = json!({
            "action": "change-public-keys",
            "publicKey": BASE64.encode(own.public_key().as_bytes()),
            "nonce": BASE64.encode(nonce),
            "clientID": client_id,
        });
        stream.write_all(request.to_string().as_bytes())?;
        let mut session = Self {
            stream,
            received: Vec::new(),
            keepassxc: SalsaBox::new(&own.public_key(), &own),
            own,
            client_id,
            wait,
            links: Vec::new(),
        };
        let answer = session.answer_to("change-public-keys", Instant::now() + wait)?;
        if answer.get("errorCode").is_some() || text(&answer, "success") != Some("true") {
            return Err(Problem::Refused);
        }
        let key = text(&answer, "publicKey")
            .and_then(|key| BASE64.decode(key).ok())
            .and_then(|bytes| PublicKey::from_slice(&bytes).ok())
            .ok_or(Problem::Garbled)?;
        session.keepassxc = SalsaBox::new(&key, &session.own);
        Ok(session)
    }

    /// The hash of the database open in front; asks KeePassXC to show its
    /// unlock window when it is locked, and waits for the user.
    fn database_hash(&mut self) -> Result<String, Problem> {
        let ask = |session: &mut Self| {
            session.request(
                "get-databasehash",
                json!({ "action": "get-databasehash" }),
                true,
            )
        };
        let answer = match ask(self) {
            Err(Reply::Error(code)) if code == DATABASE_NOT_OPENED => {
                self.wait_for("database-unlocked")?;
                ask(self)
            }
            other => other,
        }
        .map_err(Reply::into_problem)?;
        let hash = text(&answer, "hash").ok_or(Problem::Garbled)?;
        // SecretKey's words: a hash KeePassXC gives is hex.
        if hash.is_empty() || !hash.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(Problem::Garbled);
        }
        Ok(hash.chars().take(64).collect())
    }

    /// Whether KeePassXC still accepts `link` for the database in front.
    fn test_link(&mut self, link: &Link) -> Result<bool, Problem> {
        let message = json!({ "action": "test-associate", "id": link.id, "key": link.key });
        match self.request("test-associate", message, false) {
            Ok(_) => Ok(true),
            Err(Reply::Error(_)) => Ok(false),
            Err(reply) => Err(reply.into_problem()),
        }
    }

    /// Asks KeePassXC to allow KassenSturz for the database in front; the
    /// user names the connection there.
    fn associate(&mut self) -> Result<Link, Problem> {
        let id_key = BoxKey::generate(&mut OsRng);
        let key = BASE64.encode(id_key.public_key().as_bytes());
        let message = json!({
            "action": "associate",
            "key": BASE64.encode(self.own.public_key().as_bytes()),
            "idKey": key,
        });
        let answer = self
            .request("associate", message, false)
            .map_err(Reply::into_problem)?;
        let id = text(&answer, "id").ok_or(Problem::Garbled)?.to_owned();
        Ok(Link { id, key })
    }

    /// KassenSturz's entries at `url`.
    fn logins(&mut self, url: &str, link: &Link) -> Result<Vec<Entry>, Problem> {
        let message = json!({
            "action": "get-logins",
            "url": url,
            "submitUrl": url,
            "keys": [{ "id": link.id, "key": link.key }],
        });
        let mut answer = match self.request("get-logins", message, false) {
            Ok(answer) => answer,
            Err(Reply::Error(code)) if code == NO_LOGINS_FOUND => return Ok(Vec::new()),
            Err(reply) => return Err(reply.into_problem()),
        };
        let Some(Value::Array(entries)) = answer.get_mut("entries") else {
            return Ok(Vec::new());
        };
        Ok(entries
            .iter_mut()
            .filter_map(|entry| {
                let uuid = text(entry, "uuid")?.to_owned();
                let login = text(entry, "login")?.to_owned();
                let password = match entry.get_mut("password") {
                    Some(Value::String(password)) => {
                        let taken = Zeroizing::new(std::mem::take(password));
                        (!taken.is_empty()).then_some(taken)
                    }
                    _ => None,
                };
                Some(Entry {
                    uuid,
                    login,
                    password,
                })
            })
            .collect())
    }

    /// Saves `password` for `login` at `url`: a new entry, or the entry
    /// `uuid` updated, which KeePassXC asks the user to allow.
    fn set_login(
        &mut self,
        url: &str,
        link: &Link,
        login: &str,
        password: &str,
        uuid: Option<&str>,
    ) -> Result<(), Problem> {
        let mut message = json!({
            "action": "set-login",
            "url": url,
            "submitUrl": url,
            "id": link.id,
            "login": login,
            "password": password,
        });
        if let Some(uuid) = uuid {
            message["uuid"] = json!(uuid);
        }
        let result = self.request("set-login", message, false);
        result.map(|_| ()).map_err(Reply::into_problem)
    }

    /// Sends `message` encrypted and returns KeePassXC's decrypted answer.
    /// The message is wiped from memory once it is encrypted.
    fn request(&mut self, action: &str, message: Value, unlock: bool) -> Result<Value, Reply> {
        let nonce = random_nonce();
        let mut plain = Zeroizing::new(message.to_string());
        drop(scrub(message));
        let sealed = self
            .keepassxc
            .encrypt(Nonce::from_slice(&nonce), plain.as_bytes())
            .map_err(|_| Reply::Problem(Problem::Garbled))?;
        plain.zeroize();
        let mut request = Map::new();
        request.insert("action".into(), json!(action));
        request.insert("message".into(), json!(BASE64.encode(sealed)));
        request.insert("nonce".into(), json!(BASE64.encode(nonce)));
        request.insert("clientID".into(), json!(self.client_id));
        if unlock {
            request.insert("triggerUnlock".into(), json!("true"));
        }
        self.stream
            .write_all(Value::Object(request).to_string().as_bytes())
            .map_err(|_| Reply::Problem(Problem::Lost))?;
        let answer = self
            .answer_to(action, Instant::now() + self.wait)
            .map_err(Reply::Problem)?;
        if let Some(code) = answer.get("errorCode") {
            let code = match code {
                Value::String(code) => code.clone(),
                other => other.to_string(),
            };
            return Err(Reply::Error(code));
        }
        let expected = increment(nonce);
        let nonce = text(&answer, "nonce")
            .and_then(|nonce| BASE64.decode(nonce).ok())
            .filter(|nonce| nonce[..] == expected[..])
            .ok_or(Reply::Problem(Problem::Garbled))?;
        let sealed = text(&answer, "message")
            .and_then(|message| BASE64.decode(message).ok())
            .ok_or(Reply::Problem(Problem::Garbled))?;
        let plain = Zeroizing::new(
            self.keepassxc
                .decrypt(Nonce::from_slice(&nonce), sealed.as_slice())
                .map_err(|_| Reply::Problem(Problem::Garbled))?,
        );
        let answer: Value =
            serde_json::from_slice(&plain).map_err(|_| Reply::Problem(Problem::Garbled))?;
        if let Some(code) = answer.get("errorCode") {
            return Err(Reply::Error(code.as_str().unwrap_or("").to_owned()));
        }
        if text(&answer, "success").is_some_and(|success| success != "true") {
            return Err(Reply::Error(String::new()));
        }
        Ok(answer)
    }

    /// Waits for KeePassXC's notice `action`, e.g. that the user unlocked
    /// the database.
    fn wait_for(&mut self, action: &str) -> Result<(), Problem> {
        self.answer_to(action, Instant::now() + self.wait)
            .map(|_| ())
    }

    /// The next message from KeePassXC whose action is `action`; notices
    /// about other things (a database locked or unlocked) are skipped.
    fn answer_to(&mut self, action: &str, until: Instant) -> Result<Value, Problem> {
        loop {
            while let Some(message) = self.next_message()? {
                if text(&message, "action") == Some(action) {
                    return Ok(message);
                }
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Problem::Lost);
            }
            self.stream.set_read_timeout(Some(left))?;
            let mut buffer = [0_u8; 4096];
            match self.stream.read(&mut buffer) {
                Ok(0) => return Err(Problem::Lost),
                Ok(read) => self.received.extend_from_slice(&buffer[..read]),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => return Err(Problem::Lost),
            }
            if self.received.len() > MAX_ANSWER {
                return Err(Problem::Garbled);
            }
        }
    }

    /// The first whole JSON object received and not yet read, if one is
    /// complete. KeePassXC sends objects one after another, without
    /// anything between them.
    fn next_message(&mut self) -> Result<Option<Value>, Problem> {
        let start = self
            .received
            .iter()
            .position(|byte| !byte.is_ascii_whitespace());
        let Some(start) = start else {
            self.received.clear();
            return Ok(None);
        };
        let mut stream =
            serde_json::Deserializer::from_slice(&self.received[start..]).into_iter::<Value>();
        match stream.next() {
            Some(Ok(value)) => {
                let end = start + stream.byte_offset();
                self.received.drain(..end);
                Ok(Some(value))
            }
            Some(Err(error)) if error.is_eof() => Ok(None),
            None => Ok(None),
            Some(Err(_)) => Err(Problem::Garbled),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.received.zeroize();
    }
}

/// What a request got back besides an answer.
#[derive(Debug)]
enum Reply {
    /// KeePassXC's error code.
    Error(String),
    Problem(Problem),
}

impl Reply {
    fn into_problem(self) -> Problem {
        match self {
            Self::Error(_) => Problem::Refused,
            Self::Problem(problem) => problem,
        }
    }
}

/// The text under `key` of a JSON object.
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

/// Wipes the strings of a message that held a secret before it is dropped.
fn scrub(mut value: Value) -> Value {
    if let Value::Object(map) = &mut value {
        for (_, field) in map.iter_mut() {
            if let Value::String(text) = field {
                text.zeroize();
            }
        }
    }
    value
}

fn random_nonce() -> [u8; 24] {
    let nonce = SalsaBox::generate_nonce(&mut OsRng);
    let mut bytes = [0_u8; 24];
    bytes.copy_from_slice(&nonce);
    bytes
}

/// 24 random bytes in base64: a client's name for one connection.
fn random_text() -> String {
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    BASE64.encode(bytes)
}

/// The nonce plus one, as a little-endian number: what KeePassXC answers
/// with (libsodium's `sodium_increment`).
fn increment(mut nonce: [u8; 24]) -> [u8; 24] {
    for byte in &mut nonce {
        let (next, carry) = byte.overflowing_add(1);
        *byte = next;
        if !carry {
            break;
        }
    }
    nonce
}

#[cfg(test)]
mod fake;
#[cfg(test)]
mod tests;
