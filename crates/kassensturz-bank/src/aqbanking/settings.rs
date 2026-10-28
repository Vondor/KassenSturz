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
        });
    }
    records.sort_by_key(|record| record.unique_id);
    Ok(records)
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
type Value = (String, String, String);

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
        if line == "}" {
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
        let raw: String = right
            .split('"')
            .skip(1)
            .step_by(2)
            .collect::<Vec<_>>()
            .join("");
        values.push((groups.join("/"), name.to_owned(), unescape(&raw)));
    }
    values
}

fn value<'a>(values: &'a [Value], group: &str, name: &str) -> Option<&'a str> {
    values
        .iter()
        .find(|(in_group, in_name, _)| in_group == group && in_name == name)
        .map(|(_, _, value)| value.as_str())
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

    const USER_FILE: &str = r#"int uniqueId="7"
char backendName="aqhbci"
char userName="KassenSturz"
char userId="made-up-login"
char customerId="made-up-login"
char country="de"
char bankCode="37040044"
int lastSessionId="0"
data {
  backend {
    char cryptMode="pintan"
    char server="https%3A%2F%2Ffints.testbank.example%2F"
    int hbciVersion="300"
  }
}
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
        block_message_logs(&data_dir, &BankCode::new("DE", "50010517")).expect("new bank");
        assert!(bank.with_file_name("50010517").join("logs").is_file());
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
}
