// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Reviewer's local adversarial tests for D-776 (not for merge).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;

const ROWS: i64 = 400;

fn comp(path: &Path) -> Computer {
    Computer { name: "Laptop".into(), backup_folder: path.parent().unwrap().join("Backups") }
}

fn open_taking_over(path: &Path) -> Book {
    let c = comp(path);
    match Book::open(path, &c) {
        Ok(b) => b,
        Err(StoreError::Locked(seen)) => Book::open_anyway(path, &c, &seen).expect("take over stale lock"),
        Err(e) => panic!("open failed: {e:?}"),
    }
}

fn spawn(test: &str, path: &Path, extra: &[(&str, &str)]) -> Child {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env("KS_ADV_BOOK", path)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    for (k, v) in extra { cmd.env(k, v); }
    cmd.spawn().unwrap()
}

fn collect(child: &mut Child) -> Arc<Mutex<Vec<String>>> {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let out = child.stdout.take().unwrap();
    let l2 = lines.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(at) = line.find("ADV ") { l2.lock().unwrap().push(line[at..].to_owned()); }
        }
    });
    lines
}

fn setup(folder: &TempFolder) -> PathBuf {
    let path = folder.join("Household.ksz");
    let _nl = NoLocks::here();
    let book = Book::create(&path, &comp(&path)).expect("made");
    let mut book = book;
    book.use_currency(EUR).unwrap();
    for i in 0..ROWS { account(&mut book, &format!("seed{i}"), AccountKind::Asset, EUR); }
    write_gen(&mut book, 0);
    path
}

fn state(book: &Book) -> (String, i64, i64, i64) {
    let ic: String = book.conn.query_row("PRAGMA integrity_check(1)", [], |r| r.get(0)).unwrap();
    let (d, mn, n): (i64, i64, i64) = book.conn
        .query_row(&format!("SELECT count(DISTINCT {GEN}), min({GEN}), count(*) FROM accounts"), [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    (ic, d, mn, n)
}

fn gen_sql(g: i64) -> String {
    format!("UPDATE accounts SET name = printf('g%d-%d-%s', {g}, rowid, hex(randomblob(700)));")
}
fn write_gen(book: &mut Book, g: i64) {
    book.as_one_action(crate::history::Action::RenameAccount, |book| {
        book.conn.execute_batch(&gen_sql(g))?;
        Ok::<(), StoreError>(())
    }).unwrap();
}
const GEN: &str = "CAST(substr(name, 2, instr(name, '-') - 2) AS INTEGER)";

/// Child: writes generation after generation until killed.
#[test]
fn adv_child_writer() {
    let Some(path) = std::env::var_os("KS_ADV_BOOK").map(PathBuf::from) else { return };
    let _nl = (std::env::var_os("KS_ADV_LOCKS").is_none()).then(NoLocks::here);
    let mut book = open_taking_over(&path);
    let mut g: i64 = book.conn.query_row(&format!("SELECT max({GEN}) FROM accounts"), [], |r| r.get(0)).unwrap();
    println!("ADV opened {g}");
    loop {
        g += 1;
        write_gen(&mut book, g);
        println!("ADV committed {g}");
    }
}

/// Child: tries to become a second writer and says how far it got.
#[test]
fn adv_child_second() {
    let Some(path) = std::env::var_os("KS_ADV_BOOK").map(PathBuf::from) else { return };
    let _nl = (std::env::var_os("KS_ADV_LOCKS").is_none()).then(NoLocks::here);
    let c = comp(&path);
    match Book::open(&path, &c) {
        Ok(_) => println!("ADV opened-plain"),
        Err(StoreError::Locked(seen)) => match Book::open_anyway(&path, &c, &seen) {
            Ok(_) => println!("ADV opened-anyway"),
            Err(StoreError::Locked(_)) => println!("ADV refused"),
            Err(e) => println!("ADV error {e:?}"),
        },
        Err(e) => println!("ADV error {e:?}"),
    }
}

#[test]
fn adv_kill_mid_save_never_loses_a_commit() {
    if std::env::var_os("KS_ADV_BOOK").is_some() { return; }
    let folder = TempFolder::new();
    let path = setup(&folder);
    let mut rollbacks = 0;
    for i in 0..40u64 {
        let mut child = spawn("lockless::tests::adversarial::adv_child_writer", &path, &[]);
        let lines = collect(&mut child);
        std::thread::sleep(Duration::from_millis(150 + (i * 53) % 500));
        child.kill().unwrap();
        child.wait().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let last: i64 = lines.lock().unwrap().iter().rev()
            .find_map(|l| l.strip_prefix("ADV committed ").map(|g| g.parse().unwrap())).unwrap_or(-1);
        let had_journal = journal_of(&path).exists();
        if had_journal { rollbacks += 1; }
        let _nl = NoLocks::here();
        let book = open_taking_over(&path);
        let (ic, distinct, g, n) = state(&book);
        assert_eq!(ic, "ok", "round {i}: integrity");
        assert_eq!((distinct, n), (1, ROWS), "round {i}: one generation in every row");
        assert!(g >= last && g <= last + 1, "round {i}: g={g} last committed={last}");
        let j = journal_of(&path);
        if j.exists() {
            let b = std::fs::read(&j).unwrap();
            eprintln!("round {i}: journal still there after open: len={} head={:02x?}", b.len(), &b[..b.len().min(12)]);
        }
        eprintln!("round {i}: last={last} found={g} journal_was_left={had_journal}");
    }
    eprintln!("rounds with a hot journal left by the kill: {rollbacks}");
}

#[test]
fn adv_second_process_is_refused_while_first_runs() {
    if std::env::var_os("KS_ADV_BOOK").is_some() { return; }
    let folder = TempFolder::new();
    let path = setup(&folder);
    let _nl = NoLocks::here();
    let _first = open_taking_over(&path);
    let mut out = vec![];
    for locks in [false, true] {
        let extra: &[(&str, &str)] = if locks { &[("KS_ADV_LOCKS", "1")] } else { &[] };
        let mut child = spawn("lockless::tests::adversarial::adv_child_second", &path, extra);
        let lines = collect(&mut child);
        child.wait().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        out.push(format!("taker_has_locks={locks}: {:?}", lines.lock().unwrap()));
    }
    eprintln!("{out:#?}");
    assert!(out[0].contains("ADV refused"), "{out:?}");
    assert!(out[1].contains("ADV refused"), "a taker WITH locks takes over a running lockless writer: {out:?}");
}

#[test]
fn adv_reader_copy_under_continuous_writes() {
    if std::env::var_os("KS_ADV_BOOK").is_some() { return; }
    let folder = TempFolder::new();
    let path = setup(&folder);
    let mut child = spawn("lockless::tests::adversarial::adv_child_writer", &path, &[]);
    let lines = collect(&mut child);
    std::thread::sleep(Duration::from_millis(300));
    let _nl = NoLocks::here();
    let (mut ok, mut busy, mut other) = (0, 0, vec![]);
    for _ in 0..60 {
        match Book::open_to_read(&path) {
            Ok(book) => {
                let (ic, d, _, n) = state(&book);
                assert_eq!((ic.as_str(), d, n), ("ok", 1, ROWS), "a torn copy was read");
                ok += 1;
            }
            Err(StoreError::Io(std::io::ErrorKind::WouldBlock)) => busy += 1,
            Err(e) => other.push(format!("{e:?}")),
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let commits = lines.lock().unwrap().len();
    eprintln!("reads ok={ok} busy={busy} other={other:?} writer commits={commits}");
    assert!(other.is_empty());
}

#[test]
fn adv_leftover_cold_journal_blocks_readers() {
    if std::env::var_os("KS_ADV_BOOK").is_some() { return; }
    let folder = TempFolder::new();
    let path = setup(&folder);
    // Kill writers until one leaves a journal whose header is still zero
    // (killed before the journal was synced: the book itself is untouched).
    for i in 0..30u64 {
        let mut child = spawn("lockless::tests::adversarial::adv_child_writer", &path, &[]);
        std::thread::sleep(Duration::from_millis(200 + (i * 37) % 300));
        child.kill().unwrap();
        child.wait().unwrap();
        let j = journal_of(&path);
        let cold = j.exists() && std::fs::read(&j).unwrap().first() == Some(&0);
        if cold {
            // The killed program's lock file stays; readers don't care.
            let _nl = NoLocks::here();
            let mut results = vec![];
            for _ in 0..2 {
                results.push(format!("{:?}", Book::open_to_read(&path).map(|_| ())));
            }
            eprintln!("cold journal after round {i}; reads with no writer running: {results:?}");
            // The app opens and closes the book without saving.
            drop(open_taking_over(&path));
            eprintln!("after open+close without a save, journal exists: {}", j.exists());
            eprintln!("read now: {:?}", Book::open_to_read(&path).map(|_| ()));
            return;
        }
    }
    panic!("no cold journal produced");
}
