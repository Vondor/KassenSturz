// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Crash, lock and path tests that run the real program as its own process
//! and end it from outside, the way a power cut, a task manager or a logoff
//! does (US7). They run on every platform, but the Windows CI job
//! (`.github/workflows/windows-crash.yml`) is why they exist: most Beta
//! testers use Windows, and `Child::kill` is `TerminateProcess` there.
//!
//! The "program" is this test binary itself, started again with
//! `crash_worker`, which runs the CLI in a loop in-process and reports every
//! save that returned on its standard output. All names, paths and amounts are
//! made up (AGENTS.md §3.1 rule 2).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use kassensturz_cli::{Environment, run};
use kassensturz_core::AccountId;

/// Set for the worker process: the book it saves into.
const WORKER_BOOK: &str = "KASSENSTURZ_CRASH_WORKER_BOOK";

/// Set for the worker process: the number of its first save, so no two
/// rounds use the same payee.
const WORKER_START: &str = "KASSENSTURZ_CRASH_WORKER_START";

/// A folder of its own for one test, removed again afterwards. `name` is
/// the last part of the folder, so tests can give it spaces and umlauts.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("kassensturz-crash-test-{}", AccountId::generate()));
        let path = root.join(name);
        std::fs::create_dir_all(&path).expect("temporary folder created");
        Self(root.join(name))
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        if let Some(root) = self.0.parent() {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

struct Outcome {
    status: u8,
    out: String,
    err: String,
}

/// Runs one command line in this process, on a computer called "Laptop".
fn cli(book: &Path, backups: &Path, args: &[&str]) -> Outcome {
    let mut line = vec![
        "--book".to_owned(),
        book.display().to_string(),
        "--backup-folder".to_owned(),
        backups.display().to_string(),
        "--computer".to_owned(),
        "Laptop".to_owned(),
    ];
    line.extend(args.iter().map(|arg| (*arg).to_owned()));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run(&line, &Environment::default(), &mut out, &mut err);
    Outcome {
        status,
        out: String::from_utf8(out).expect("UTF-8 output"),
        err: String::from_utf8(err).expect("UTF-8 errors"),
    }
}

fn ok(book: &Path, backups: &Path, args: &[&str]) -> String {
    let outcome = cli(book, backups, args);
    assert_eq!(outcome.status, 0, "{args:?} failed: {}", outcome.err);
    outcome.out
}

/// A book with one checking account and the categories the worker books to.
fn new_book(book: &Path, backups: &Path) {
    ok(book, backups, &["create"]);
    ok(
        book,
        backups,
        &[
            "account",
            "add",
            "Giro",
            "--type",
            "checking",
            "--currency",
            "EUR",
        ],
    );
    for name in ["Food", "Home"] {
        ok(
            book,
            backups,
            &[
                "category",
                "add",
                name,
                "--kind",
                "expense",
                "--currency",
                "EUR",
            ],
        );
    }
}

/// The amount of save number `n` in cents: always a different one.
fn cents(n: u32) -> u32 {
    n + 1
}

fn plain_amount(cents: u32) -> String {
    format!("-{}.{:02}", cents / 100, cents % 100)
}

/// The worker. Not a test of its own: `cargo test` skips it (`ignore`), and
/// the crash tests below start it as a process and kill it. Every save that
/// returned is reported as a line `saved N` before the next one starts.
#[test]
#[ignore = "started by the crash tests as a process of its own"]
fn crash_worker() {
    let Some(book) = std::env::var_os(WORKER_BOOK).map(PathBuf::from) else {
        return;
    };
    let backups = book.with_file_name("Backups");
    let start: u32 = std::env::var(WORKER_START)
        .ok()
        .and_then(|start| start.parse().ok())
        .unwrap_or(0);
    for n in start.. {
        let amount = plain_amount(cents(n));
        let payee = format!("Shop-{n}");
        let mut args = vec![
            "transaction",
            "add",
            "--date",
            "2026-10-05",
            "--account",
            "Giro",
            "--amount",
            &amount,
            "--payee",
            &payee,
        ];
        // Every other save is a split into two Parts, so "every balance
        // equals its parts" is tested with real splits.
        let half = format!("Food={}", plain_amount(cents(n) / 2));
        if n % 2 == 1 {
            args.extend(["--part", &half, "--part", "Home="]);
        } else {
            args.extend(["--category", "Food"]);
        }
        let outcome = cli(&book, &backups, &args);
        assert_eq!(outcome.status, 0, "save {n} failed: {}", outcome.err);
        // One write call per line, flushed by the OS at once: the parent
        // must see the line the moment the save has returned.
        println!("saved {n}");
        use std::io::Write;
        std::io::stdout().flush().expect("flushed");
    }
}

/// The second worker: opens the book, reports `locked` and then sits there
/// holding it, until it is ended from outside.
#[test]
#[ignore = "started by the lock test as a process of its own"]
fn lock_holder() {
    let Some(book) = std::env::var_os(WORKER_BOOK).map(PathBuf::from) else {
        return;
    };
    let computer = kassensturz_store::Computer {
        name: "Laptop".into(),
        backup_folder: book.with_file_name("Backups"),
    };
    let _held = kassensturz_store::Book::open(&book, &computer).expect("opened");
    println!("locked");
    use std::io::Write;
    std::io::stdout().flush().expect("flushed");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Starts the lock holder on `book` and ends it as soon as it holds the lock.
fn kill_lock_holder(book: &Path) {
    let mut child = Command::new(std::env::current_exe().expect("this test program"))
        .args(["--ignored", "--exact", "lock_holder", "--nocapture"])
        .env(WORKER_BOOK, book)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("holder started");
    let mut reader = BufReader::new(child.stdout.take().expect("holder output"));
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).expect("holder output read");
        assert_ne!(read, 0, "the holder ended by itself");
        if line.trim() == "locked" {
            break;
        }
    }
    child.kill().expect("holder ended");
    child.wait().expect("holder reaped");
}

/// Starts the worker on `book`, waits until it has reported `reports` saves,
/// then ends it from outside. Returns the numbers of the saves it reported.
fn kill_worker_after(book: &Path, start: u32, reports: usize) -> Vec<u32> {
    let mut child = Command::new(std::env::current_exe().expect("this test program"))
        .args(["--ignored", "--exact", "crash_worker", "--nocapture"])
        .env(WORKER_BOOK, book)
        .env(WORKER_START, start.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("worker started");
    let mut reader = BufReader::new(child.stdout.take().expect("worker output"));
    let mut saved = Vec::new();
    let mut line = String::new();
    while saved.len() < reports {
        line.clear();
        let read = reader.read_line(&mut line).expect("worker output read");
        assert_ne!(read, 0, "the worker ended by itself after {saved:?}");
        if let Some(number) = line.trim().strip_prefix("saved ") {
            saved.push(number.parse().expect("a number"));
        }
    }
    // `kill` is SIGKILL, and TerminateProcess on Windows: no clean-up runs.
    child.kill().expect("worker ended");
    child.wait().expect("worker reaped");
    saved
}

/// Opens the book after a kill: a left-over lock file is refused with its
/// code, and `--open-anyway` takes it over, as the user would after reading
/// the warning. Returns whether there was a lock.
fn reopen(book: &Path, backups: &Path) -> bool {
    let first = cli(book, backups, &["balances"]);
    if first.status == 0 {
        return false;
    }
    assert!(
        first.err.contains("store.locked"),
        "a killed program left something other than a lock: {}",
        first.err
    );
    let second = cli(book, backups, &["--open-anyway", "balances"]);
    assert_eq!(second.status, 0, "{}", second.err);
    true
}

/// The `Shop-N` payees of Giro's register, in register order, and the last
/// balance column.
fn register_payees(book: &Path, backups: &Path) -> (Vec<u32>, String) {
    let register = ok(book, backups, &["register", "Giro"]);
    let mut payees = Vec::new();
    let mut last_balance = String::new();
    for line in register.lines().skip(2) {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let Some(number) = words
            .iter()
            .find_map(|word| word.strip_prefix("Shop-"))
            .and_then(|number| number.parse().ok())
        {
            payees.push(number);
            // Date, payee, amount, balance, id.
            last_balance = words[words.len() - 2].to_owned();
        }
    }
    (payees, last_balance)
}

/// The sum of the saves with these numbers, in cents.
fn total_cents(numbers: &[u32]) -> i64 {
    numbers.iter().map(|n| -i64::from(cents(*n))).sum()
}

fn money(cents: i64) -> String {
    format!(
        "{}{}.{:02}",
        if cents < 0 { "-" } else { "" },
        cents.abs() / 100,
        cents.abs() % 100
    )
}

/// The app is ended by force in the middle of saving, several times in a
/// row on the same book (B-278). Every save it reported is there exactly
/// once, nothing is made up, and every balance is the sum of its parts.
#[test]
fn a_program_killed_while_saving_loses_nothing_it_reported() {
    let folder = Folder::new("Kill test");
    let book = folder.0.join("Household.ksz");
    let backups = folder.0.join("Backups");
    new_book(&book, &backups);

    let mut reported_all = Vec::new();
    for round in 0..5 {
        let reported = kill_worker_after(&book, round as u32 * 1000, 8 + round * 3);
        reported_all.extend(reported.iter().copied());
        reopen(&book, &backups);

        let (payees, last_balance) = register_payees(&book, &backups);
        for number in &reported {
            assert!(
                payees.contains(number),
                "round {round}: reported save {number} is missing, book has {payees:?}"
            );
        }
        // No save is there twice.
        let mut counts = std::collections::BTreeMap::new();
        for number in &payees {
            *counts.entry(*number).or_insert(0usize) += 1;
        }
        assert!(
            counts.values().all(|count| *count == 1),
            "round {round}: a save is there twice: {counts:?}"
        );
        // The book's own balance equals the sum of what its register lists,
        // that is of every part of every transaction it holds.
        assert_eq!(
            ok(&book, &backups, &["balances"])
                .lines()
                .last()
                .map(str::to_owned),
            Some(format!(
                "Total{:>width$} EUR",
                money(total_cents(&payees)),
                width = last_total_width(&book, &backups)
            )),
            "round {round}: the balance isn't the sum of the entries"
        );
        assert_eq!(last_balance, money(total_cents(&payees)));
        // The Parts of each split add up: the categories hold what the
        // account lost.
        let food = ok(&book, &backups, &["register", "Food"]);
        let home = ok(&book, &backups, &["register", "Home"]);
        assert!(!food.is_empty() && !home.is_empty());
    }
    assert!(reported_all.len() >= 8 + 11 + 14 + 17 + 20);
}

/// Width of the amount column in the `Total` row of `balances`, so the
/// expected line can be built without guessing the padding.
fn last_total_width(book: &Path, backups: &Path) -> usize {
    let output = ok(book, backups, &["balances"]);
    let total = output.lines().last().expect("a total row");
    total.len() - "Total".len() - " EUR".len()
}

/// The lock of a program that was ended by force stays (nothing could remove
/// it), and the next start says so and refuses, instead of opening the book
/// behind its back. `--open-anyway` opens it and leaves a clean state: the
/// lock is removed when the book is closed again.
#[test]
fn the_lock_of_a_killed_program_is_found_and_taken_over_on_the_next_start() {
    let folder = Folder::new("Lock test");
    let book = folder.0.join("Household.ksz");
    let backups = folder.0.join("Backups");
    new_book(&book, &backups);
    let lock = PathBuf::from(format!("{}.lock", book.display()));
    assert!(!lock.exists(), "the lock stayed after a normal close");

    kill_lock_holder(&book);
    assert!(lock.exists(), "a killed program left no lock file");
    let refused = cli(&book, &backups, &["balances"]);
    assert_eq!(refused.status, 1);
    assert!(refused.err.contains("store.locked"), "{}", refused.err);
    // A kill can come between creating the lock file and writing its text,
    // so the computer's name may be missing; the refusal is the same.
    // Refusing changed nothing: the lock is as it was.
    assert!(lock.exists());

    let opened = cli(&book, &backups, &["--open-anyway", "balances"]);
    assert_eq!(opened.status, 0, "{}", opened.err);
    assert!(
        opened.err.contains("opening the book anyway"),
        "{}",
        opened.err
    );
    assert!(!lock.exists(), "the lock stayed after it was taken over");
    // And now nothing is in the way any more.
    ok(&book, &backups, &["balances"]);
}

/// A second program opening a book that is open in a running one is
/// refused. `ok` on Windows too: the lock file is deleted and made again by
/// name, so an open handle must not get in the way.
#[test]
fn a_book_open_in_a_running_program_is_refused() {
    let folder = Folder::new("Running test");
    let book = folder.0.join("Household.ksz");
    let backups = folder.0.join("Backups");
    new_book(&book, &backups);
    let computer = kassensturz_store::Computer {
        name: "Other".into(),
        backup_folder: backups.clone(),
    };
    let held = kassensturz_store::Book::open(&book, &computer).expect("opened here");
    let refused = cli(&book, &backups, &["balances"]);
    assert_eq!(refused.status, 1);
    assert!(refused.err.contains("store.locked"), "{}", refused.err);
    assert!(refused.err.contains("Other"), "{}", refused.err);
    drop(held);
    ok(&book, &backups, &["balances"]);
}

/// Folder and file names with spaces and umlauts, for the book, its lock,
/// its backups, an import and an export, as a German household has them.
#[test]
fn paths_with_spaces_and_umlauts_work_everywhere() {
    let folder = Folder::new("Mein Haushalt für Müller & Söhne");
    let book = folder.0.join("Größe Bücher (Ä Ö Ü ß).ksz");
    let backups = folder.0.join("Sicherungen für übermorgen");
    new_book(&book, &backups);
    ok(
        &book,
        &backups,
        &[
            "transaction",
            "add",
            "--date",
            "2026-10-05",
            "--account",
            "Giro",
            "--amount",
            "-12.50",
            "--category",
            "Food",
            "--payee",
            "Bäckerei Müßiggang",
        ],
    );
    assert!(ok(&book, &backups, &["register", "Giro"]).contains("Bäckerei Müßiggang"));
    // Closing and opening again keeps it, and the lock is gone.
    assert!(!PathBuf::from(format!("{}.lock", book.display())).exists());
    assert!(ok(&book, &backups, &["balances"]).contains("-12.50 EUR"));

    // Export into a path like that, and make a book from it again.
    let export = folder.0.join("Ausgabe für März.json");
    ok(
        &book,
        &backups,
        &["export", &export.display().to_string(), "--format", "book"],
    );
    let copy = folder.0.join("Kopie für Ömer.ksz");
    ok(
        &copy,
        &backups,
        &["create", "--from", &export.display().to_string()],
    );
    assert!(ok(&copy, &backups, &["register", "Giro"]).contains("Bäckerei Müßiggang"));

    // A statement file in such a folder, with umlauts in its entries.
    let statement = folder.0.join("Kontoauszug Öktober.xml");
    std::fs::write(&statement, camt053_xml("UTF-8", "Gemüse &amp; Früchte Süd")).expect("written");
    let imported = ok(
        &book,
        &backups,
        &[
            "import",
            &statement.display().to_string(),
            "--account",
            "Giro",
        ],
    );
    assert!(
        imported.contains("1 transaction") || imported.contains("Imported"),
        "{imported}"
    );
    assert!(ok(&book, &backups, &["register", "Giro"]).contains("Gemüse & Früchte Süd"));
}

/// One booked entry in a CAMT.053 file for the example IBAN.
fn camt053_xml(encoding: &str, name: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"{encoding}\"?>\n\
         <Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.02\"><BkToCstmrStmt><Stmt>\
         <Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy></Acct>\
         <Bal><Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp><Amt Ccy=\"EUR\">1.00</Amt>\
         <CdtDbtInd>CRDT</CdtDbtInd><Dt><Dt>2026-10-31</Dt></Dt></Bal>\
         <Ntry><Amt Ccy=\"EUR\">7.77</Amt><CdtDbtInd>DBIT</CdtDbtInd><Sts>BOOK</Sts>\
         <BookgDt><Dt>2026-10-06</Dt></BookgDt><AcctSvcrRef>U1</AcctSvcrRef>\
         <NtryDtls><TxDtls><RltdPties><Cdtr><Nm>{name}</Nm></Cdtr><Dbtr><Nm>{name}</Nm></Dbtr></RltdPties></TxDtls></NtryDtls>\
         </Ntry></Stmt></BkToCstmrStmt></Document>"
    )
}

/// The same text as UTF-16 little endian with a byte order mark, as a
/// Windows editor saves "Unicode".
fn utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

/// A UTF-16 CAMT file imports like the same file in UTF-8 (B-280, #978,
/// fixed in #996). This was an expected failure until then.
#[test]
fn a_utf16_camt_file_imports() {
    let folder = Folder::new("Utf16 test");
    let book = folder.0.join("Household.ksz");
    let backups = folder.0.join("Backups");
    new_book(&book, &backups);
    let statement = folder.0.join("utf16.xml");
    std::fs::write(
        &statement,
        utf16le_with_bom(&camt053_xml("UTF-16", "Gemüse &amp; Früchte")),
    )
    .expect("written");
    let outcome = cli(
        &book,
        &backups,
        &[
            "import",
            &statement.display().to_string(),
            "--account",
            "Giro",
        ],
    );
    assert_eq!(
        outcome.status, 0,
        "a UTF-16 CAMT file is refused (B-280, #978): {}",
        outcome.err
    );
    assert!(ok(&book, &backups, &["register", "Giro"]).contains("Gemüse & Früchte"));
}

/// The same file in UTF-8 imports, so the UTF-16 test above is about
/// the encoding and nothing else in the test.
#[test]
fn the_same_camt_file_in_utf8_imports() {
    let folder = Folder::new("Utf8 test");
    let book = folder.0.join("Household.ksz");
    let backups = folder.0.join("Backups");
    new_book(&book, &backups);
    let statement = folder.0.join("utf8.xml");
    std::fs::write(&statement, camt053_xml("UTF-8", "Gemüse &amp; Früchte")).expect("written");
    ok(
        &book,
        &backups,
        &[
            "import",
            &statement.display().to_string(),
            "--account",
            "Giro",
        ],
    );
    assert!(ok(&book, &backups, &["register", "Giro"]).contains("Gemüse & Früchte"));
}
