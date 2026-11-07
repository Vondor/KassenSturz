// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the backend reads from AqBanking's own folder (D-139), and what it
//! removes there.
//!
//! AqBanking 6.9.2 exports no function that lists the logins it has set
//! up (D-144: the user API is hidden), but it keeps each one as a small
//! text file, `settings6/users/<id in hex>.conf`, and each account as
//! `settings6/accounts/<id in hex>.conf`, in Gwenhywfar's "DB" format:
//! `type name="value"` lines inside named groups. The backend reads the
//! few values it needs from them (the IDs, the bank code, the country,
//! how the user logs in, which login an account belongs to) and nothing
//! else: the login name stays in the file (threat T-19).
//!
//! AqBanking also writes every FinTS message to `logs` folders below
//! `backends/aqhbci/data/banks/`, with the PIN blanked but the statements
//! and the FinTS registration number readable, and has no switch to stop
//! it. [`block_message_logs`] stops it anyway, before every bank contact:
//! a plain file named `logs` where AqBanking wants the folder makes it
//! skip the log (D-261). [`delete_message_logs`] still removes any `logs`
//! folder, e.g. from an older KassenSturz (D-139, threat T-06).

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::backend::TanMethodChoice;
use crate::iban::BankCode;

/// AqBanking's settings folder below its data folder.
const SETTINGS: &str = "settings6";

/// A login as its settings file describes it, without the login name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UserRecord {
    /// AqBanking's unique ID of the login.
    pub unique_id: u32,
    /// The bank's country, lower case as AqBanking keeps it.
    pub country: String,
    /// The bank code.
    pub bank_code: String,
    /// How the user logs in, as the FinTS backend names it: `pintan`,
    /// `ddv` (chip card), `rdh` or `rah` (key files, D-059).
    pub crypt_mode: Option<String>,
    /// The bank's FinTS address the login uses.
    pub server: Option<String>,
    /// The FinTS version as AqBanking keeps it (`300`, `220`, …).
    pub hbci_version: Option<String>,
    /// The TAN method selected for the login (`selectedTanMethod`), as the
    /// number `setitanmode` takes (D-617).
    pub selected_tan_method: Option<u16>,
    /// The TAN methods the bank allows this login (`tanMethodList`, set by
    /// `getitanmodes` or the bank's answer), as numbers.
    pub tan_methods: Vec<u16>,
    /// The methods the bank's parameters name for this login, each with
    /// the FinTS version of the TAN segment it is offered in (D-615).
    pub tan_choices: Vec<TanMethodChoice>,
}

impl UserRecord {
    /// The methods to offer for this login (D-615): those of the bank's
    /// parameters that are on the login's list of allowed ones, each once in
    /// the highest version the bank offers it in, those of version 6 and up
    /// first (only they take the dialog path that strong customer
    /// authentication needs, AqBanking's `cbox_dialog.c`); or, when the
    /// parameters name none, the numbers of the list alone. The placeholder
    /// 999 (unknown) is left out.
    pub(crate) fn offered_methods(&self) -> Vec<TanMethodChoice> {
        let allowed: Vec<u16> = self
            .tan_methods
            .iter()
            .copied()
            .filter(|method| *method != 999)
            .collect();
        let mut choices: Vec<TanMethodChoice> = Vec::new();
        for choice in &self.tan_choices {
            if !allowed.is_empty() && !allowed.contains(&choice.function) {
                continue;
            }
            match choices
                .iter_mut()
                .find(|known| known.function == choice.function)
            {
                Some(known) if known.version < choice.version => *known = choice.clone(),
                Some(_) => {}
                None => choices.push(choice.clone()),
            }
        }
        choices.sort_by_key(|choice| choice.version < 6);
        if !choices.is_empty() {
            return choices;
        }
        allowed
            .into_iter()
            .map(|function| TanMethodChoice {
                number: function,
                function,
                version: 0,
                name: String::new(),
            })
            .collect()
    }
}

/// The logins AqBanking has set up, by unique ID.
pub(crate) fn users(data_dir: &Path) -> io::Result<Vec<UserRecord>> {
    let mut records = Vec::new();
    for text in read_group(data_dir, "users")? {
        let values = parse(&text);
        let Some(unique_id) = value(&values, "", "uniqueId").and_then(|id| id.parse().ok()) else {
            continue;
        };
        records.push(UserRecord {
            unique_id,
            country: value(&values, "", "country").unwrap_or("de").to_owned(),
            bank_code: value(&values, "", "bankCode")
                .unwrap_or_default()
                .to_owned(),
            crypt_mode: value(&values, "data/backend", "cryptMode").map(str::to_owned),
            server: value(&values, "data/backend", "server").map(str::to_owned),
            hbci_version: value(&values, "data/backend", "hbciVersion").map(str::to_owned),
            selected_tan_method: value(&values, "data/backend", "selectedTanMethod")
                .and_then(|method| method.trim().parse().ok()),
            tan_methods: list(&values, "data/backend", "tanMethodList")
                .iter()
                .filter_map(|method| method.trim().parse().ok())
                .collect(),
            tan_choices: tan_choices(&text),
        });
    }
    records.sort_by_key(|record| record.unique_id);
    // The same login in two files is one login (M6-B185).
    records.dedup_by_key(|record| record.unique_id);
    Ok(records)
}

/// The login name of the login with `unique_id` (`userId` at the top of its
/// file). It is read only for the screen that changes a login (D-617, a
/// narrow exception to threat T-19) and is not part of [`UserRecord`], whose
/// `Debug` could otherwise carry it into a log.
pub(crate) fn login_name(data_dir: &Path, unique_id: u32) -> io::Result<Option<String>> {
    for text in read_group(data_dir, "users")? {
        let values = parse(&text);
        if value(&values, "", "uniqueId").and_then(|id| id.parse::<u32>().ok()) == Some(unique_id) {
            return Ok(value(&values, "", "userId").map(str::to_owned));
        }
    }
    Ok(None)
}

/// The longest method name kept, in characters.
const MAX_NAME: usize = 80;

/// The TAN methods in a login's bank parameters (`bpd/bpdjobs/HITANS/<version>/
/// tanMethod` groups, D-615), each once, in file order. A method comes with
/// the name the bank gave it (`methodName`, else `methodId`, else
/// `zkaTanName`); the text is the bank's, so control characters are dropped
/// and it is cut at [`MAX_NAME`] characters. Groups that aren't under a
/// numbered `HITANS` version or have no usable number are skipped.
fn tan_choices(text: &str) -> Vec<TanMethodChoice> {
    #[derive(Default)]
    struct Method {
        version: u16,
        process: u16,
        function: Option<u16>,
        names: [Option<String>; 3],
    }
    let mut choices: Vec<TanMethodChoice> = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    let mut current: Option<Method> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('}') {
            if let Some(name) = groups.pop()
                && name == "tanMethod"
                && let Some(method) = current.take()
                && let Some(function) = method.function
                // AqBanking uses processes 1 and 2 only (applayer/cbox_itan.c).
                && matches!(method.process, 1 | 2)
            {
                let [method_name, method_id, zka] = method.names;
                let name: String = method_name
                    .or(method_id)
                    .or(zka)
                    .unwrap_or_default()
                    .chars()
                    .filter(|character| {
                // Control characters, and the marks that turn text around
                // (U+202A to U+202E, U+2066 to U+2069), would let a name
                // look like something else on screen.
                !character.is_control()
                    && !matches!(*character, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
            })
                    .take(MAX_NAME)
                    .collect();
                // The range is checked before the number is put together, so
                // a bank's oversized number can't overflow.
                let number = crate::backend::TAN_METHODS
                    .contains(&function)
                    .then(|| method.version * 1000 + function);
                if let Some(number) = number
                    && crate::backend::valid_tan_method(number)
                    && !choices.iter().any(|known| known.number == number)
                {
                    choices.push(TanMethodChoice {
                        number,
                        function,
                        version: method.version,
                        name: name.trim().to_owned(),
                    });
                }
            }
            continue;
        }
        if let Some(name) = line.strip_suffix('{') {
            let name = name.trim().to_owned();
            if name == "tanMethod"
                && groups.len() >= 2
                && groups[groups.len() - 2] == "HITANS"
                && let Ok(version) = groups[groups.len() - 1].parse::<u16>()
                && (1..=9).contains(&version)
            {
                current = Some(Method {
                    version,
                    ..Method::default()
                });
            }
            groups.push(name);
            continue;
        }
        let (Some(method), Some((left, right))) = (current.as_mut(), line.split_once('=')) else {
            continue;
        };
        if groups.last().map(String::as_str) != Some("tanMethod") {
            continue;
        }
        let key = left.split_whitespace().last().unwrap_or_default();
        let raw: String = right.split('"').skip(1).step_by(2).collect();
        let value = unescape(&raw);
        match key {
            "function" => method.function = value.trim().parse().ok(),
            "process" => method.process = value.trim().parse().unwrap_or(0),
            "methodName" => method.names[0] = Some(value),
            "methodId" => method.names[1] = Some(value),
            "zkaTanName" => method.names[2] = Some(value),
            _ => {}
        }
    }
    choices
}

/// How many login settings files the folder holds, how many of them give
/// a login ID and how many of those are PIN/TAN logins: counts only, for
/// the bank log when a lookup finds no login (M6-B181, D-607).
#[cfg(feature = "debug-log")]
pub(crate) fn user_file_counts(data_dir: &Path) -> io::Result<(u32, u32, u32)> {
    let (mut files, mut with_id, mut pin_tan) = (0_u32, 0_u32, 0_u32);
    for text in read_group(data_dir, "users")? {
        files += 1;
        let values = parse(&text);
        if value(&values, "", "uniqueId").is_some() {
            with_id += 1;
        }
        if value(&values, "data/backend", "cryptMode") == Some("pintan") {
            pin_tan += 1;
        }
    }
    Ok((files, with_id, pin_tan))
}

/// Which login each account AqBanking knows belongs to: the account's
/// unique ID to the login's.
pub(crate) fn account_owners(data_dir: &Path) -> io::Result<HashMap<u32, u32>> {
    let mut owners = HashMap::new();
    for text in read_group(data_dir, "accounts")? {
        let values = parse(&text);
        let account = value(&values, "", "uniqueId").and_then(|id| id.parse().ok());
        let user = value(&values, "", "userId").and_then(|id| id.parse().ok());
        if let (Some(account), Some(user)) = (account, user) {
            owners.insert(account, user);
        }
    }
    Ok(owners)
}

/// The text of every `.conf` file in one settings group. A folder that
/// doesn't exist yet holds nothing.
fn read_group(data_dir: &Path, group: &str) -> io::Result<Vec<String>> {
    let folder = data_dir.join(SETTINGS).join(group);
    let entries = match fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "conf")
        })
        .collect();
    files.sort();
    files.iter().map(fs::read_to_string).collect()
}

/// One value of a settings file: the group path (`data/backend`, empty
/// at the top), the name and the value with Gwenhywfar's escapes undone.
type Value = (String, String, String, Vec<String>);

/// Reads Gwenhywfar's "DB" text format as far as the backend needs it:
/// `name {` opens a group, `}` closes it, `type name="value"` sets a
/// value; a value can be written as several quoted parts. Anything else
/// is skipped. Values are percent-escaped (`%2F`), which is undone.
fn parse(text: &str) -> Vec<Value> {
    let mut values = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Gwenhywfar closes every group with `} #name` (detailed groups),
        // older files with a bare `}`.
        if line.starts_with('}') {
            groups.pop();
            continue;
        }
        if let Some(name) = line.strip_suffix('{') {
            groups.push(name.trim().to_owned());
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        let name = left.split_whitespace().last().unwrap_or_default();
        let parts: Vec<String> = right.split('"').skip(1).step_by(2).map(unescape).collect();
        let raw: String = right
            .split('"')
            .skip(1)
            .step_by(2)
            .collect::<Vec<_>>()
            .join("");
        values.push((groups.join("/"), name.to_owned(), unescape(&raw), parts));
    }
    values
}

fn value<'a>(values: &'a [Value], group: &str, name: &str) -> Option<&'a str> {
    values
        .iter()
        .find(|(in_group, in_name, _, _)| in_group == group && in_name == name)
        .map(|(_, _, value, _)| value.as_str())
}

/// Every value of a variable that holds a list, such as
/// `int tanMethodList="920", "920"`, from one line or several.
fn list(values: &[Value], group: &str, name: &str) -> Vec<String> {
    values
        .iter()
        .filter(|(in_group, in_name, _, _)| in_group == group && in_name == name)
        .flat_map(|(_, _, _, parts)| parts.iter().cloned())
        .collect()
}

/// Undoes Gwenhywfar's escaping of values: `%XX` is the byte XX.
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Removes every `logs` folder below AqBanking's bank data (D-139) and
/// returns how many files went. A folder that doesn't exist holds no logs.
pub(crate) fn delete_message_logs(data_dir: &Path) -> io::Result<usize> {
    let banks = data_dir
        .join("backends")
        .join("aqhbci")
        .join("data")
        .join("banks");
    let mut removed = 0;
    if banks.is_dir() {
        remove_logs_below(&banks, &mut removed)?;
    }
    Ok(removed)
}

/// Makes AqBanking write no message log for `bank` (D-261, threat T-06):
/// a plain, empty file `logs` takes the place of the folder AqBanking
/// writes its logs into, `banks/<country>/<code>/logs/` below its data,
/// and a `logs` folder already there is removed. AqBanking 6.9.2 makes
/// that folder with `GWEN_Directory_GetPath` before each log
/// (`AH_Msg_LogMessage`), which fails when a part of the path is a file
/// (Gwenhywfar 5.14.1), and then logs nothing. A bank code that isn't
/// letters and digits is no folder name and is refused.
pub(crate) fn block_message_logs(data_dir: &Path, bank: &BankCode) -> io::Result<()> {
    let plain = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric());
    if !plain(bank.country()) || !plain(bank.code()) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let folder = data_dir
        .join("backends")
        .join("aqhbci")
        .join("data")
        .join("banks")
        .join(bank.country().to_ascii_lowercase())
        .join(bank.code());
    fs::create_dir_all(&folder)?;
    let logs = folder.join("logs");
    if logs.is_dir() {
        fs::remove_dir_all(&logs)?;
    }
    if !logs.is_file() {
        fs::write(&logs, "")?;
    }
    Ok(())
}

/// Takes away the block of [`block_message_logs`] for `bank`, for the one
/// call of level 3 of the bank log (D-620): the plain file `logs` goes, and
/// AqBanking makes its folder again. The next call that doesn't run at level
/// 3 blocks it anew, and [`delete_message_logs`] removes what was written.
#[cfg(feature = "debug-log")]
pub(crate) fn allow_message_logs(data_dir: &Path, bank: &BankCode) -> io::Result<()> {
    let plain = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric());
    if !plain(bank.country()) || !plain(bank.code()) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let logs = data_dir
        .join("backends")
        .join("aqhbci")
        .join("data")
        .join("banks")
        .join(bank.country().to_ascii_lowercase())
        .join(bank.code())
        .join("logs");
    if logs.is_file() {
        fs::remove_file(&logs)?;
    }
    Ok(())
}

/// The statement-request and TAN segments (`HKCAZ`, `HKKAZ`, `HKTAN`) in the
/// message logs below AqBanking's bank data, oldest first, at most the last
/// [`MAX_SEGMENTS`] of them, as plain text. Only for the shape that level 3
/// of the bank log notes (D-620), which `shape_segment` makes of each before
/// it is written anywhere; the logs themselves are removed right after by
/// [`delete_message_logs`].
#[cfg(feature = "debug-log")]
pub(crate) fn message_log_segments(data_dir: &Path) -> Vec<String> {
    let banks = data_dir
        .join("backends")
        .join("aqhbci")
        .join("data")
        .join("banks");
    let mut files = Vec::new();
    collect_log_files(&banks, false, &mut files);
    files.sort();
    let mut segments = Vec::new();
    for file in files {
        let Ok(bytes) = fs::read(&file) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        segments.extend(
            split_segments(&text)
                .filter(|segment| kassensturz_core::bank_log::is_noted_segment(segment)),
        );
    }
    let skip = segments.len().saturating_sub(MAX_SEGMENTS);
    segments.split_off(skip)
}

/// How many segments [`message_log_segments`] returns at most.
#[cfg(feature = "debug-log")]
const MAX_SEGMENTS: usize = 40;

/// The files in the `logs` folders below `folder`.
#[cfg(feature = "debug-log")]
fn collect_log_files(folder: &Path, in_logs: bool, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let logs = in_logs || path.file_name().is_some_and(|name| name == "logs");
            collect_log_files(&path, logs, files);
        } else if in_logs {
            files.push(path);
        }
    }
}

/// The segments of a FinTS message text: cut at every `'` that is not
/// escaped by `?`, white space at the start of a piece dropped.
#[cfg(feature = "debug-log")]
fn split_segments(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if c == '?' {
            current.push(c);
            escaped = true;
        } else if c == '\'' {
            pieces.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    pieces.push(current);
    pieces
        .into_iter()
        .map(|piece| piece.trim_start().to_owned())
}

fn remove_logs_below(folder: &Path, removed: &mut usize) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|name| name == "logs") {
            *removed += fs::read_dir(&path)?.count();
            fs::remove_dir_all(&path)?;
        } else {
            remove_logs_below(&path, removed)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A login file in the format AqBanking writes: the certificate group
    /// comes first and every group closes with `} #name` (Gwenhywfar's
    /// detailed groups, M6-B184). Everything in it is made up.
    const USER_FILE: &str = r#"int uniqueId="7"
char backendName="aqhbci"
char userName="KassenSturz"
char userId="made-up-login"
char customerId="made-up-login"
char country="de"
char bankCode="37040044"
int lastSessionId="0"
certDb {
  cert {
    char serial="made-up"
  } #cert
} #certDb
data {
  backend {
    char cryptMode="pintan"
    char server="https%3A%2F%2Ffints.testbank.example%2F"
    int hbciVersion="300"
    int selectedTanMethod="6912"
    int tanMethodList="920", "912"
    int tanMethodList="999"
    bpd {
      bpdjobs {
        HITANS {
          6 {
            tanMethod {
              int function="912"
              int process="2"
              char methodName="Test list"
            } #tanMethod
            tanMethod {
              int function="920"
              int process="2"
              char methodName="Test app (old)"
            } #tanMethod
            tanMethod {
              int function="930"
              int process="9"
              char methodName="Test not used"
            } #tanMethod
          } #6
          7 {
            tanMethod {
              char methodName="Test app"
              int function="920"
              int process="2"
            } #tanMethod
          } #7
        } #HITANS
      } #bpdjobs
    } #bpd
  } #backend
} #data
"#;

    #[test]
    fn a_user_file_gives_the_id_the_bank_and_the_login_kind() {
        let values = parse(USER_FILE);
        assert_eq!(value(&values, "", "uniqueId"), Some("7"));
        assert_eq!(value(&values, "", "bankCode"), Some("37040044"));
        assert_eq!(value(&values, "data/backend", "cryptMode"), Some("pintan"));
        assert_eq!(
            value(&values, "data/backend", "server"),
            Some("https://fints.testbank.example/")
        );
        assert_eq!(value(&values, "", "cryptMode"), None);
    }

    #[test]
    fn a_real_user_file_is_read_past_its_certificate_group() {
        // M6-B184: before the fix every value after `certDb` was filed under
        // a wrong path, so no login and no method was found.
        let values = parse(USER_FILE);
        assert_eq!(value(&values, "data/backend", "cryptMode"), Some("pintan"));
        assert_eq!(
            backend_pin_tan_count(&values),
            1,
            "found where AqBanking puts it"
        );
        let choices = tan_choices(USER_FILE);
        let seen: Vec<(u16, &str)> = choices
            .iter()
            .map(|choice| (choice.number, choice.name.as_str()))
            .collect();
        // Process 9 is not used by AqBanking and is left out.
        assert_eq!(
            seen,
            vec![
                (6912, "Test list"),
                (6920, "Test app (old)"),
                (7920, "Test app"),
            ]
        );
        assert_eq!(choices[2].function, 920);
        assert_eq!(choices[2].version, 7);
    }

    fn backend_pin_tan_count(values: &[Value]) -> usize {
        values
            .iter()
            .filter(|(group, name, value, _)| {
                group == "data/backend" && name == "cryptMode" && value == "pintan"
            })
            .count()
    }

    #[test]
    fn the_offered_methods_are_each_once_in_their_highest_version() {
        let data_dir = folder("offered");
        let users = data_dir.join(SETTINGS).join("users");
        fs::create_dir_all(&users).expect("folder");
        fs::write(users.join("00000007.conf"), USER_FILE).expect("file");
        let records = users_in(&data_dir);
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.crypt_mode.as_deref(), Some("pintan"));
        assert_eq!(
            record.server.as_deref(),
            Some("https://fints.testbank.example/")
        );
        let methods = record.offered_methods();
        let offered: Vec<(u16, &str)> = methods
            .iter()
            .map(|choice| (choice.number, choice.name.as_str()))
            .collect();
        assert_eq!(offered, vec![(6912, "Test list"), (7920, "Test app")]);
    }

    fn users_in(data_dir: &Path) -> Vec<UserRecord> {
        users(data_dir).expect("users")
    }

    #[test]
    fn odd_numbers_and_names_and_older_files_are_handled_not_trusted() {
        // A bank's oversized number can't overflow, and a name turned around
        // by direction marks is cleaned; a bare `}` closer still works.
        let odd = "bpd {\nbpdjobs {\nHITANS {\n9 {\ntanMethod {\nint function=\"60000\"\nint process=\"2\"\n} #tanMethod\n\
            tanMethod {\nint function=\"930\"\nint process=\"1\"\nchar methodName=\"a\u{202E}b\u{2066}c\"\n}\n} #9\n} #HITANS\n}\n}\n";
        let odd = tan_choices(odd);
        assert_eq!(odd.len(), 1);
        assert_eq!(odd[0].number, 9930);
        assert_eq!(odd[0].name, "abc");
        assert!(tan_choices("int uniqueId=\"7\"\n").is_empty());
    }

    #[test]
    fn values_in_several_parts_and_odd_lines_are_read_or_skipped() {
        let values = parse("char name=\"a\" \"b\"\n# comment\n\nnonsense\nint n=\"5\"\n");
        assert_eq!(value(&values, "", "name"), Some("ab"));
        assert_eq!(value(&values, "", "n"), Some("5"));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn escapes_are_undone_and_broken_ones_kept() {
        assert_eq!(unescape("a%20b"), "a b");
        assert_eq!(unescape("100%"), "100%");
        assert_eq!(unescape("%zz"), "%zz");
        assert_eq!(unescape("%C3%A9"), "é");
    }

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "kassensturz-bank-settings-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn users_and_account_owners_come_from_the_settings_files() {
        let data_dir = folder("users");
        let users_dir = data_dir.join(SETTINGS).join("users");
        fs::create_dir_all(&users_dir).expect("created");
        fs::write(users_dir.join("00000007.conf"), USER_FILE).expect("written");
        fs::write(
            users_dir.join("00000002.conf"),
            "int uniqueId=\"2\"\nchar bankCode=\"10000000\"\ndata {\n backend {\n char cryptMode=\"ddv\"\n }\n}\n",
        )
        .expect("written");
        fs::write(users_dir.join("notes.txt"), "not a settings file").expect("written");
        let accounts_dir = data_dir.join(SETTINGS).join("accounts");
        fs::create_dir_all(&accounts_dir).expect("created");
        fs::write(
            accounts_dir.join("00000003.conf"),
            "int uniqueId=\"3\"\nint userId=\"7\"\n",
        )
        .expect("written");
        let users = users(&data_dir).expect("read");
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].unique_id, 2);
        assert_eq!(users[0].crypt_mode.as_deref(), Some("ddv"));
        assert_eq!(users[1].unique_id, 7);
        assert_eq!(users[1].bank_code, "37040044");
        assert_eq!(users[1].country, "de");
        assert_eq!(users[1].crypt_mode.as_deref(), Some("pintan"));
        assert_eq!(
            users[1].server.as_deref(),
            Some("https://fints.testbank.example/")
        );
        assert_eq!(users[1].hbci_version.as_deref(), Some("300"));
        assert_eq!(users[1].tan_methods, vec![920, 912, 999]);
        assert!(users[0].tan_methods.is_empty());
        assert_eq!(users[0].server, None);
        assert_eq!(account_owners(&data_dir).expect("read").get(&3), Some(&7));
        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn a_missing_folder_holds_nothing() {
        let data_dir = folder("missing");
        assert_eq!(users(&data_dir).expect("read"), vec![]);
        assert!(account_owners(&data_dir).expect("read").is_empty());
        assert_eq!(delete_message_logs(&data_dir).expect("nothing"), 0);
    }

    #[test]
    fn message_logs_are_removed_and_the_rest_kept() {
        let data_dir = folder("logs");
        let bank = data_dir
            .join("backends")
            .join("aqhbci")
            .join("data")
            .join("banks")
            .join("de")
            .join("37040044");
        fs::create_dir_all(bank.join("logs")).expect("created");
        fs::write(bank.join("logs").join("20261023.log"), "HNHBK").expect("written");
        fs::write(bank.join("logs").join("20261022.log"), "HNHBK").expect("written");
        fs::write(bank.join("keep.txt"), "not a log").expect("written");
        assert_eq!(delete_message_logs(&data_dir).expect("removed"), 2);
        assert!(!bank.join("logs").exists());
        assert!(bank.join("keep.txt").exists());
        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn a_file_named_logs_takes_the_place_of_the_log_folder_and_stays() {
        let data_dir = folder("blocked");
        let bank = data_dir
            .join("backends")
            .join("aqhbci")
            .join("data")
            .join("banks")
            .join("de")
            .join("37040044");
        // A log folder AqBanking wrote before is removed with its logs.
        fs::create_dir_all(bank.join("logs")).expect("created");
        fs::write(bank.join("logs").join("20261028.log"), "HNHBK").expect("written");
        let code = BankCode::new("DE", "37040044");
        block_message_logs(&data_dir, &code).expect("blocked");
        assert!(bank.join("logs").is_file());
        assert_eq!(fs::read(bank.join("logs")).expect("read"), b"");
        // Again, and after the cleaning: it stays a file.
        block_message_logs(&data_dir, &code).expect("blocked again");
        assert_eq!(delete_message_logs(&data_dir).expect("cleaned"), 0);
        assert!(bank.join("logs").is_file());
        // A bank never contacted before gets its folders made.
        block_message_logs(&data_dir, &BankCode::new("DE", "10000000")).expect("new bank");
        assert!(bank.with_file_name("10000000").join("logs").is_file());
        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn a_bank_code_that_isnt_a_folder_name_is_refused() {
        let data_dir = folder("refused");
        for code in [
            BankCode::new("DE", ""),
            BankCode::new("DE", "../x"),
            BankCode::new("D/", "37040044"),
            BankCode::new("DE", "3704 0044"),
        ] {
            assert!(block_message_logs(&data_dir, &code).is_err(), "{code:?}");
        }
        assert!(!data_dir.exists());
    }

    #[cfg(feature = "debug-log")]
    #[test]
    fn the_segments_come_out_of_the_message_logs_and_only_the_wanted_ones() {
        let dir = std::env::temp_dir().join(format!("ks-seg-{}", std::process::id()));
        let logs = dir.join("backends/aqhbci/data/banks/de/12345678/logs");
        fs::create_dir_all(&logs).unwrap();
        // Made-up message text.
        fs::write(
            logs.join("a.log"),
            "HNHBK:1:3+000+300'HNSHK:2:4+PIN:2'HKCAZ:3:1+DE00:XX+urn?:x+N+20260101'\nHKTAN:4:7+4+HKCAZ?'x'HNSHA:5:2+1+1234'",
        )
        .unwrap();
        let segments = message_log_segments(&dir);
        assert_eq!(
            segments,
            [
                "HKCAZ:3:1+DE00:XX+urn?:x+N+20260101",
                "HKTAN:4:7+4+HKCAZ?'x"
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
        assert!(message_log_segments(&dir).is_empty());
    }

    #[cfg(feature = "debug-log")]
    #[test]
    fn level_three_takes_the_block_away_and_the_next_call_puts_it_back() {
        let dir = std::env::temp_dir().join(format!("ks-allow-{}", std::process::id()));
        let bank = BankCode::new("DE", "12345678");
        block_message_logs(&dir, &bank).unwrap();
        let logs = dir.join("backends/aqhbci/data/banks/de/12345678/logs");
        assert!(logs.is_file());
        allow_message_logs(&dir, &bank).unwrap();
        assert!(!logs.exists());
        // Nothing to take away is no error.
        allow_message_logs(&dir, &bank).unwrap();
        block_message_logs(&dir, &bank).unwrap();
        assert!(logs.is_file());
        fs::remove_dir_all(&dir).unwrap();
    }
}
