// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How changes prepared outside the window reach it (M5.10, D-216): a
//! folder of request files in KassenSturz's data folder.
//!
//! The user's own AI tool asks `kassensturz-mcp` to prepare changes. That
//! program can't change the book (it reads it without a lock, D-195), so it
//! hands the call's arguments to the running app as a request file and
//! waits for the answer file. The app checks the arguments itself with
//! [`crate::propose`] against its open book, shows the set in its window,
//! and only the window's confirm button applies it (D-200). Nothing here
//! opens a socket: the folder is the whole way in (Q-124 A, D-175). The app
//! reaches requests only through [`Inbox`], so the part that shows,
//! confirms and applies a set knows nothing about files, and a later socket
//! replaces only this module.
//!
//! The files, in one folder that only the user may enter (0700, files
//! 0600 on Unix), each named by the request's ID:
//! - `<id>.request.json`: written by the MCP server, the book and the call;
//! - `<id>.taken.json`: the same file once an app took it. Taking is a
//!   rename, so only one app takes a request, and the server can tell
//!   whether it withdrew a request before any app saw it;
//! - `<id>.answer.json`: the app's latest answer ([`Reply`]).
//!
//! Each file is written to a hidden temporary file first and renamed, so a
//! reader never sees half of one. The server removes a request's files
//! once it has read the final answer; the app removes whatever is older
//! than [`MAX_AGE`].

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

/// The request folder's name in KassenSturz's data folder.
pub const FOLDER_NAME: &str = "mcp-requests";

/// KassenSturz's data folder's name in the user's data folder: the app ID
/// (D-049), as the app and the command-line tool use it.
pub const APP_FOLDER: &str = "io.github.vondor.KassenSturz";

/// The largest request file an app reads. A call of `propose_changes` with
/// its most changes is far smaller.
pub const MAX_REQUEST_BYTES: u64 = 512 * 1024;

/// How long a request's files may stay before the app removes them, also
/// when nobody read the answer.
pub const MAX_AGE: Duration = Duration::from_secs(60 * 60);

/// The version of the request file's layout.
const VERSION: u64 = 1;

const REQUEST: &str = ".request.json";
const TAKEN: &str = ".taken.json";
const ANSWER: &str = ".answer.json";
const TEMPORARY: &str = ".tmp";

/// A request's ID: 32 lowercase hexadecimal digits, which is also the
/// start of each of its files' names.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(String);

impl RequestId {
    /// A new ID, ordered by time.
    fn new() -> Self {
        Self(uuid::Uuid::now_v7().simple().to_string())
    }

    /// The ID written as `text`, if it is one.
    pub fn parse(text: &str) -> Option<Self> {
        (text.len() == 32
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        .then(|| Self(text.to_owned()))
    }

    /// The ID as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn file(&self, ending: &str) -> String {
        format!("{}{ending}", self.0)
    }
}

/// A call of `propose_changes` or `propose_view` from outside the window,
/// for the book it names. Its arguments are untrusted: the app checks them
/// with [`crate::prepare_named`] like a call from its own chat (X-8).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// Its ID, for the answer.
    pub id: RequestId,
    /// The book the call was prepared against.
    pub book: PathBuf,
    /// The function called: `propose_changes`, or `propose_view` (D-352).
    /// A request file without one is a call of `propose_changes`, as every
    /// file was before M6c.8.
    pub function: &'static str,
    /// The call's arguments, as the model sent them.
    pub arguments: Value,
}

/// The app's answer to a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// The set waits in the window for the user: `changes` changes.
    Shown {
        /// How many changes the set holds.
        changes: usize,
    },
    /// The user confirmed the set and it was applied, as one change undo
    /// takes back.
    Applied,
    /// The user threw the set away. Nothing changed.
    Declined,
    /// Nothing changed, for the reason `code` (an error code, D-034).
    Failed {
        /// The error code.
        code: String,
    },
}

impl Reply {
    /// Whether the user or the app has decided: no other answer follows.
    pub fn is_final(&self) -> bool {
        !matches!(self, Self::Shown { .. })
    }

    /// The answer as the answer file holds it.
    pub fn to_json(&self) -> Value {
        match self {
            Self::Shown { changes } => json!({"status": "shown", "changes": changes}),
            Self::Applied => json!({"status": "applied"}),
            Self::Declined => json!({"status": "declined"}),
            Self::Failed { code } => json!({"status": "failed", "error": code}),
        }
    }

    /// The answer an answer file holds, if it is one.
    pub fn from_json(value: &Value) -> Option<Self> {
        match value.get("status")?.as_str()? {
            "shown" => Some(Self::Shown {
                changes: usize::try_from(value.get("changes")?.as_u64()?).ok()?,
            }),
            "applied" => Some(Self::Applied),
            "declined" => Some(Self::Declined),
            "failed" => Some(Self::Failed {
                code: value.get("error")?.as_str()?.to_owned(),
            }),
            _ => None,
        }
    }
}

/// Where a request stands, as the MCP server sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Standing {
    /// No app has taken it yet.
    NotTaken,
    /// An app took it and hasn't answered yet.
    Taken,
    /// The app's latest answer.
    Answered(Reply),
    /// There is no such request (any more).
    Unknown,
}

/// How the window gets requests and answers them, whatever carries them
/// (D-216): the request folder now, maybe a socket later (Q-124).
pub trait Inbox {
    /// The requests for the book at `book` that arrived since the last
    /// call, each taken so that no other app shows it. A request that
    /// can't be read is answered with `mcp-writes.bad-request` here.
    fn take_new(&mut self, book: &Path) -> Vec<Request>;

    /// Answers the request `id`. A later answer replaces an earlier one.
    fn reply(&mut self, id: &RequestId, reply: &Reply) -> io::Result<()>;
}

/// The request folder.
#[derive(Debug)]
pub struct RequestFolder {
    path: PathBuf,
    /// The requests this app left alone because they are for another
    /// book, so it reads each only once.
    ignored: HashSet<RequestId>,
}

impl RequestFolder {
    /// Where the request folder is on this computer: in KassenSturz's data
    /// folder, `$XDG_DATA_HOME/io.github.vondor.KassenSturz/mcp-requests`
    /// (by default under `~/.local/share`). The Flatpak gives the app and
    /// every program started with `flatpak run --command=…` the same
    /// `XDG_DATA_HOME`, so they meet there without a new permission
    /// (D-219). `None` off Linux until the Windows and macOS packaging
    /// steps (M7) say where their data goes, like the AI switch (D-177).
    pub fn default_place() -> Option<PathBuf> {
        if cfg!(not(target_os = "linux")) {
            return None;
        }
        let nonempty = |name| std::env::var_os(name).filter(|value| !value.is_empty());
        let data = nonempty("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| nonempty("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
        Some(data.join(APP_FOLDER).join(FOLDER_NAME))
    }

    /// Opens the folder at `path`, making it if needed, for the user only.
    /// A folder that others may enter is closed to them first; a link or a
    /// file in its place is refused.
    pub fn open(path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        let builder = {
            let mut builder = fs::DirBuilder::new();
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = fs::DirBuilder::new();
        match builder.create(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the request folder is not a folder",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            }
        }
        Ok(Self {
            path,
            ignored: HashSet::new(),
        })
    }

    /// The folder.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Hands the call `arguments` for the book at `book` to the app: the
    /// MCP server's side.
    pub fn send(&self, book: &Path, arguments: &Value) -> io::Result<RequestId> {
        self.send_call(book, crate::PROPOSE_CHANGES.name, arguments)
    }

    /// Hands the call of `function` (`propose_changes` or `propose_view`)
    /// with `arguments` for the book at `book` to the app.
    pub fn send_call(
        &self,
        book: &Path,
        function: &str,
        arguments: &Value,
    ) -> io::Result<RequestId> {
        let book = book.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "the book's path isn't UTF-8")
        })?;
        let id = RequestId::new();
        let request = json!({
            "version": VERSION,
            "book": book,
            "function": function,
            "arguments": arguments,
        });
        self.write(&id.file(REQUEST), request.to_string().as_bytes())?;
        Ok(id)
    }

    /// Takes the request `id` back if no app has taken it yet. Returns
    /// whether it was still there: if not, an app has it.
    pub fn withdraw(&self, id: &RequestId) -> io::Result<bool> {
        match fs::remove_file(self.path.join(id.file(REQUEST))) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Where the request `id` stands.
    pub fn standing(&self, id: &RequestId) -> Standing {
        if let Some(reply) = read_small(&self.path.join(id.file(ANSWER)))
            .and_then(|text| serde_json::from_str(&text).ok())
            .and_then(|value: Value| Reply::from_json(&value))
        {
            return Standing::Answered(reply);
        }
        if is_file(&self.path.join(id.file(REQUEST))) {
            Standing::NotTaken
        } else if is_file(&self.path.join(id.file(TAKEN))) {
            Standing::Taken
        } else {
            Standing::Unknown
        }
    }

    /// Removes every file of the request `id`, once its final answer was
    /// read.
    pub fn finish(&self, id: &RequestId) {
        for ending in [REQUEST, TAKEN, ANSWER] {
            let _ = fs::remove_file(self.path.join(id.file(ending)));
        }
    }

    /// Writes `bytes` as the file `name`, through a temporary file that is
    /// renamed, so nobody reads half of it.
    fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let temporary = self.path.join(format!(".{name}{TEMPORARY}"));
        let _ = fs::remove_file(&temporary);
        let mut options = OpenOptions::new();
        // A new file only: never one that is there already, or a link.
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let written = options.open(&temporary).and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        });
        let renamed = written.and_then(|()| fs::rename(&temporary, self.path.join(name)));
        if renamed.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        renamed
    }

    /// Removes the files older than [`MAX_AGE`] at `now`.
    fn clean(&self, entries: &[(String, fs::Metadata)], now: SystemTime) {
        for (name, metadata) in entries {
            let old = metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > MAX_AGE);
            if old && own_file(name) {
                let _ = fs::remove_file(self.path.join(name));
            }
        }
    }

    /// The app's side of [`Inbox::take_new`] at `now`.
    pub fn take_new_at(&mut self, book: &Path, now: SystemTime) -> Vec<Request> {
        let Ok(listing) = fs::read_dir(&self.path) else {
            return Vec::new();
        };
        let mut entries: Vec<(String, fs::Metadata)> = listing
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().into_string().ok()?;
                // The entry's own metadata: a link is a link, not its target.
                let metadata = entry.metadata().ok()?;
                Some((name, metadata))
            })
            .collect();
        self.clean(&entries, now);
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let ours = fs::canonicalize(book).ok();
        let mut taken = Vec::new();
        for (name, metadata) in &entries {
            let Some(id) = name.strip_suffix(REQUEST).and_then(RequestId::parse) else {
                continue;
            };
            if !metadata.is_file() || self.ignored.contains(&id) {
                continue;
            }
            match self.read_request(&id, metadata) {
                Some(request) => {
                    let same_book = ours.is_some()
                        && fs::canonicalize(&request.book).ok().as_ref() == ours.as_ref();
                    if !same_book {
                        // Another app may have that book open; this one
                        // leaves it alone (D-216).
                        self.ignored.insert(id);
                        continue;
                    }
                    if self.take(&id) {
                        taken.push(request);
                    }
                }
                None => {
                    if self.take(&id) {
                        let _ = self.reply(
                            &id,
                            &Reply::Failed {
                                code: "mcp-writes.bad-request".to_owned(),
                            },
                        );
                    }
                }
            }
        }
        taken
    }

    /// The request file of `id`, if it can be read: small enough, JSON of
    /// the known layout.
    fn read_request(&self, id: &RequestId, metadata: &fs::Metadata) -> Option<Request> {
        if metadata.len() > MAX_REQUEST_BYTES {
            return None;
        }
        let text = read_small(&self.path.join(id.file(REQUEST)))?;
        let value: Value = serde_json::from_str(&text).ok()?;
        if value.get("version")?.as_u64()? != VERSION {
            return None;
        }
        let book = PathBuf::from(value.get("book")?.as_str()?);
        // Only the two functions that prepare changes; anything else is not
        // a request this app reads.
        let function = match value.get("function") {
            None => crate::PROPOSE_CHANGES.name,
            Some(name) => [crate::PROPOSE_CHANGES.name, crate::PROPOSE_VIEW_NAME]
                .into_iter()
                .find(|known| name.as_str() == Some(known))?,
        };
        let arguments = value.get("arguments")?.clone();
        Some(Request {
            id: id.clone(),
            book,
            function,
            arguments,
        })
    }

    /// Takes the request `id` for this app; false when another app or the
    /// server was first.
    fn take(&self, id: &RequestId) -> bool {
        fs::rename(
            self.path.join(id.file(REQUEST)),
            self.path.join(id.file(TAKEN)),
        )
        .is_ok()
    }
}

impl Inbox for RequestFolder {
    fn take_new(&mut self, book: &Path) -> Vec<Request> {
        self.take_new_at(book, SystemTime::now())
    }

    fn reply(&mut self, id: &RequestId, reply: &Reply) -> io::Result<()> {
        self.write(&id.file(ANSWER), reply.to_json().to_string().as_bytes())
    }
}

/// Whether `name` is one of the files this module writes.
fn own_file(name: &str) -> bool {
    let name = name
        .strip_prefix('.')
        .and_then(|name| name.strip_suffix(TEMPORARY))
        .unwrap_or(name);
    [REQUEST, TAKEN, ANSWER].iter().any(|ending| {
        name.strip_suffix(ending)
            .and_then(RequestId::parse)
            .is_some()
    })
}

/// Whether `path` is a file, not a link to one.
fn is_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// The text of the file at `path`, if it is a file (not a link) of at most
/// [`MAX_REQUEST_BYTES`].
fn read_small(path: &Path) -> Option<String> {
    if !is_file(path) {
        return None;
    }
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    (u64::try_from(text.len()).ok()? <= MAX_REQUEST_BYTES).then_some(text)
}

#[cfg(test)]
mod tests;
