// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: CC0-1.0

//! An example importer plugin for KassenSturz (#791 step 9, docs/PLUGINS.md).
//!
//! It reads the text statements of the invented "Example Bank", one of
//! which is `sample.txt` beside this file:
//!
//! ```text
//! EXAMPLE-BANK STATEMENT
//! Account: DE89370400440532013000
//! Currency: EUR
//! Opening: 2026-10-31 1000.00
//! 2026-11-01 | Example Bakery | Bread | -23.45
//! Closing: 2026-11-01 976.55
//! ```
//!
//! Copy this folder to start a plugin of your own. The parts that every
//! plugin has are marked "Every plugin"; the parts that are only about the
//! Example Bank's file are in `read` and below it.
//!
//! The plugin has no imports and uses no floating point: an amount is read
//! as text and written as whole minor units, again as text (AGENTS.md §3.1).

#![no_std]

extern crate alloc;

use alloc::string::String;
use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write as _;
use core::sync::atomic::{AtomicUsize, Ordering};

// ---- Every plugin: the manifest ---------------------------------------

/// The manifest, as the custom section `kassensturz-manifest`. It is the
/// file `manifest.json` beside this one, byte for byte.
const MANIFEST_BYTES: &[u8] = include_bytes!("../manifest.json");

#[unsafe(link_section = "kassensturz-manifest")]
#[used]
pub static MANIFEST: [u8; MANIFEST_BYTES.len()] = *include_bytes!("../manifest.json");

// ---- Every plugin: memory and the way bytes go in and out -------------

unsafe extern "C" {
    /// Where the memory the program doesn't use itself begins (set by the
    /// linker).
    static __heap_base: u8;
}

const PAGE: usize = 65_536;

/// The next free address. A plugin is called by one thread at a time.
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A bump allocator: it hands out the next free bytes and never frees. That
/// is enough here, because `ks_alloc` starts again at the beginning for each
/// call, once the host has copied the previous answer out.
struct Bump;

unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut at = NEXT.load(Ordering::Relaxed);
        if at == 0 {
            at = core::ptr::addr_of!(__heap_base) as usize;
        }
        let start = at.next_multiple_of(layout.align());
        let Some(end) = start.checked_add(layout.size()) else {
            return core::ptr::null_mut();
        };
        let have = core::arch::wasm32::memory_size(0) * PAGE;
        if end > have {
            // memory.grow answers usize::MAX when the host's limit is hit.
            let pages = (end - have).div_ceil(PAGE);
            if core::arch::wasm32::memory_grow(0, pages) == usize::MAX {
                return core::ptr::null_mut();
            }
        }
        NEXT.store(end, Ordering::Relaxed);
        start as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: Bump = Bump;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // A trap: the host stops the plugin and says so.
    core::arch::wasm32::unreachable()
}

/// Gives the host room for `len` bytes of input and returns the address.
/// The host calls it before every call, so the memory of the call before is
/// free again.
#[unsafe(no_mangle)]
pub extern "C" fn ks_alloc(len: i32) -> i32 {
    NEXT.store(0, Ordering::Relaxed);
    let Ok(layout) = Layout::from_size_align(usize::try_from(len).unwrap_or(0), 1) else {
        return 0;
    };
    // SAFETY: the layout has a size and an alignment of 1.
    unsafe { ALLOCATOR.alloc(layout) as i32 }
}

/// The input of a call, as the host put it into memory.
fn input<'a>(ptr: i32, len: i32) -> &'a [u8] {
    if len <= 0 {
        return &[];
    }
    // SAFETY: the host wrote `len` bytes at `ptr`, which it got from `ks_alloc`.
    unsafe { core::slice::from_raw_parts(ptr as usize as *const u8, len as usize) }
}

/// Hands an answer to the host: its address in the high 32 bits, its length
/// in the low 32 bits. The answer stays in memory until the next call.
fn answer(text: String) -> i64 {
    let text = text.leak();
    ((text.as_ptr() as usize as i64) << 32) | text.len() as i64
}

// ---- The importer's three calls (docs/PLUGINS.md §8) ------------------

/// Which file name extensions the file dialog offers.
#[unsafe(no_mangle)]
pub extern "C" fn ks_importer_info(_ptr: i32, _len: i32) -> i64 {
    answer(String::from(r#"{"extensions": ["txt"]}"#))
}

/// Says whether the file looks like this plugin's format, from its first
/// bytes: "yes" when it is sure, "maybe" when it can't tell, "no" otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn ks_importer_recognize(ptr: i32, len: i32) -> i64 {
    let start = input(ptr, len);
    let recognition = if start.starts_with(HEADER.as_bytes()) {
        "yes"
    } else {
        "no"
    };
    answer(alloc::format!(r#"{{"recognition": "{recognition}"}}"#))
}

/// Reads the whole file: the statements, or "malformed" with the line.
#[unsafe(no_mangle)]
pub extern "C" fn ks_importer_read(ptr: i32, len: i32) -> i64 {
    match read(input(ptr, len)) {
        Ok(statements) => answer(statements),
        Err(line) => answer(alloc::format!(
            r#"{{"error": "malformed", "line": {line}}}"#
        )),
    }
}

// ---- The Example Bank's file ------------------------------------------

const HEADER: &str = "EXAMPLE-BANK STATEMENT";

/// The currencies this plugin knows, all with 2 decimal places (ISO 4217).
/// An amount is written in whole minor units, so the plugin must know how
/// many places a currency has, and refuses a currency it doesn't know
/// rather than guess.
const CURRENCIES: [&str; 4] = ["EUR", "USD", "GBP", "CHF"];

/// Reads a statement into the answer of `ks_importer_read`, or gives the
/// number of the first line it can't read (counted from 1).
fn read(bytes: &[u8]) -> Result<String, usize> {
    let text = core::str::from_utf8(bytes).map_err(|_| 1_usize)?;
    // Blank lines are skipped; the others are trimmed.
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty());

    let (number, first) = lines.next().ok_or(1_usize)?;
    if first != HEADER {
        return Err(number);
    }
    let (number, line) = lines.next().ok_or(number)?;
    let iban = line.strip_prefix("Account:").ok_or(number)?.trim();
    if iban.is_empty() || !iban.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(number);
    }
    let (number, line) = lines.next().ok_or(number)?;
    let currency = line.strip_prefix("Currency:").ok_or(number)?.trim();
    if !CURRENCIES.contains(&currency) {
        return Err(number);
    }
    let (number, line) = lines.next().ok_or(number)?;
    let opening = balance(line, "Opening:").ok_or(number)?;

    let mut entries = String::new();
    let mut closing = None;
    let mut last = number;
    for (number, line) in lines {
        last = number;
        if closing.is_some() {
            // Nothing may follow the closing balance.
            return Err(number);
        }
        if line.starts_with("Closing:") {
            closing = Some(balance(line, "Closing:").ok_or(number)?);
            continue;
        }
        if !entries.is_empty() {
            entries.push(',');
        }
        entry(line, &mut entries).ok_or(number)?;
    }
    let closing = closing.ok_or(last + 1)?;

    let mut out = String::from(r#"{"statements": [{"account": {"iban": "#);
    // The IBAN holds only letters and digits, so this can't refuse it.
    let _ = string(&mut out, iban);
    out.push_str(r#"}, "currency": ""#);
    out.push_str(currency);
    out.push_str(r#"", "opening_balance": "#);
    opening.write(&mut out);
    out.push_str(r#", "closing_balance": "#);
    closing.write(&mut out);
    out.push_str(r#", "entries": ["#);
    out.push_str(&entries);
    out.push_str("]}]}");
    Ok(out)
}

/// A balance: its date and its amount in minor units, as text.
struct Balance<'a> {
    date: &'a str,
    amount: String,
}

impl Balance<'_> {
    fn write(&self, out: &mut String) {
        // Dates and amounts hold only digits and "-", so they need no escaping.
        let _ = write!(
            out,
            r#"{{"amount": "{}", "date": "{}"}}"#,
            self.amount, self.date
        );
    }
}

/// `Opening: 2026-10-31 1000.00`
fn balance<'a>(line: &'a str, label: &str) -> Option<Balance<'a>> {
    let (date, amount) = line.strip_prefix(label)?.trim().split_once(' ')?;
    Some(Balance {
        date: iso_date(date)?,
        amount: minor_units(amount.trim())?,
    })
}

/// `2026-11-01 | Example Bakery | Bread | -23.45`: writes the entry as JSON.
fn entry(line: &str, out: &mut String) -> Option<()> {
    let mut fields = line.split('|').map(str::trim);
    let date = iso_date(fields.next()?)?;
    let name = fields.next()?;
    let memo = fields.next()?;
    let amount = minor_units(fields.next()?)?;
    if fields.next().is_some() {
        return None;
    }
    // The Example Bank only lists what is booked.
    let _ = write!(
        out,
        r#"{{"status": "booked", "booking_date": "{date}", "amount": "{amount}""#
    );
    if !name.is_empty() {
        out.push_str(r#", "other_party_name": "#);
        if !string(out, name) {
            return None;
        }
    }
    if !memo.is_empty() {
        out.push_str(r#", "remittance_information": "#);
        if !string(out, memo) {
            return None;
        }
    }
    out.push('}');
    Some(())
}

/// `2026-11-01` in its shape. Whether the day exists is for KassenSturz to
/// check: it refuses a date like 2026-02-30 when it reads the answer.
fn iso_date(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let shaped = bytes.len() == 10
        && bytes.iter().enumerate().all(|(at, byte)| match at {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        });
    shaped.then_some(text)
}

/// `-23.45` as `"-2345"`: whole minor units, as text, with no leading zeros.
/// The file always has two decimal places, as every currency in `CURRENCIES`.
/// No number type is involved, so nothing is ever rounded.
fn minor_units(text: &str) -> Option<String> {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (whole, cents) = rest.split_once('.')?;
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    // At most 15 whole digits keep the amount within 64 bits.
    if whole.is_empty() || whole.len() > 15 || !digits(whole) || cents.len() != 2 || !digits(cents)
    {
        return None;
    }
    let mut all = String::new();
    all.push_str(whole);
    all.push_str(cents);
    let trimmed = all.trim_start_matches('0');
    if trimmed.is_empty() {
        // Zero has no sign.
        return Some(String::from("0"));
    }
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(trimmed);
    Some(out)
}

/// Writes `text` as a JSON string. Returns false for a control character,
/// which a bank text has no business holding.
fn string(out: &mut String, text: &str) -> bool {
    if text.chars().any(char::is_control) {
        return false;
    }
    out.push('"');
    for character in text.chars() {
        if character == '"' || character == '\\' {
            out.push('\\');
        }
        out.push(character);
    }
    out.push('"');
    true
}
