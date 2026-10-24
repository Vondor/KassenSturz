// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A stand-in for AqBanking and Gwenhywfar in tests (threat T-09, D-055).
//!
//! The crate builds a shared library that exports every function the
//! bindings in `kassensturz-aqbanking-sys` declare, so a test loads it in
//! place of `libgwenhywfar.so.79` and `libaqbanking.so.44` and drives the
//! real safe wrapper and the real bank backend through the C ABI. The
//! functions here play **scenarios**: a bank answering well, a wrong PIN,
//! a TAN request as text (`tan`) or as a picture (`optical-tan`), a
//! request to approve in the bank's app (`approval`), each at the fetch or,
//! with the prefix `setup-`, at the setup's `getsysid`, an unreachable
//! server, a refused certificate, and
//! malformed data of every kind the wrapper must survive. The rest of the
//! functions are generated defaults that answer nothing (`generated.rs`,
//! made by `generate.py`).
//!
//! The scenario's name is read from the file
//! `kassensturz-aqbanking-stub-<pid>` in the temporary folder at
//! `GWEN_Init`; the setup commands and fetches are written to
//! `ks-stub-journal.txt` in the data folder, so a test can check what the
//! backend asked for. **No PIN is ever written**: the journal notes only
//! how many characters it had (T-01). Everything here is made up; the
//! IBANs are SWIFT's examples.
//!
//! Objects handed to the caller are leaked on purpose: the process is a
//! test and ends soon, and a dangling pointer would make a test flaky.

#![allow(
    non_snake_case,
    missing_docs, // the C functions are documented by AqBanking's headers
    clippy::missing_safety_doc,
    clippy::not_unsafe_ptr_arg_deref
)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::PathBuf;
use std::ptr;
use std::sync::{Mutex, MutexGuard};

use kassensturz_aqbanking_sys::aqbanking::*;
use kassensturz_aqbanking_sys::gwenhywfar::*;

mod generated;

/// The journal file in the data folder.
pub const JOURNAL_FILE: &str = "ks-stub-journal.txt";

/// The bank the scenarios play: a made-up bank with Deutsche Bank's
/// example bank code from SWIFT's IBAN examples.
pub const BANK_CODE: &str = "37040044";
/// The current account's IBAN (SWIFT's German example).
pub const CURRENT_IBAN: &str = "DE89370400440532013000";
/// The current account's number.
pub const CURRENT_NUMBER: &str = "0532013000";
/// The credit card's number, as the bank shows it.
pub const CARD_NUMBER: &str = "1234567890";
/// The other party's IBAN in the fetched transactions (SWIFT's example
/// for Germany with a different bank).
pub const OTHER_IBAN: &str = "DE02120300000000202051";

struct Account {
    unique_id: u32,
    user_id: u32,
    kind: c_int,
    iban: Option<&'static str>,
    number: &'static str,
    name: &'static str,
}

struct State {
    scenario: String,
    data_dir: Option<PathBuf>,
    gui: *mut Gui,
    next_user_id: u32,
    next_account_id: u32,
    accounts: Vec<Account>,
}

// SAFETY: the pointer is only used on the thread that set it.
unsafe impl Send for State {}

static STATE: Mutex<State> = Mutex::new(State {
    scenario: String::new(),
    data_dir: None,
    gui: ptr::null_mut(),
    next_user_id: 1,
    next_account_id: 1,
    accounts: Vec::new(),
});

fn state() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn journal(line: &str) {
    let state = state();
    if let Some(dir) = &state.data_dir {
        let mut text = std::fs::read_to_string(dir.join(JOURNAL_FILE)).unwrap_or_default();
        text.push_str(line);
        text.push('\n');
        let _ = std::fs::write(dir.join(JOURNAL_FILE), text);
    }
}

fn leak_text(text: &str) -> *const c_char {
    CString::new(text).map_or(ptr::null(), |text| {
        Box::leak(text.into_boxed_c_str()).as_ptr()
    })
}

fn leak_bytes(bytes: &[u8]) -> *const c_char {
    let mut owned = bytes.to_vec();
    owned.push(0);
    Box::leak(owned.into_boxed_slice())
        .as_ptr()
        .cast::<c_char>()
}

fn leak<T>(value: T) -> *mut T {
    Box::into_raw(Box::new(value))
}

unsafe fn text<'a>(pointer: *const c_char) -> Option<&'a str> {
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the caller passes a terminated string.
        unsafe { CStr::from_ptr(pointer) }.to_str().ok()
    }
}

// --- Gwenhywfar: start-up, versions, GUI ---------------------------------

/// The GUI the wrapper creates: only the callbacks it sets.
pub struct Gui {
    get_password: GWEN_GUI_GETPASSWORD_FN,
    set_password_status: GWEN_GUI_SETPASSWORDSTATUS_FN,
    log_hook: GWEN_GUI_LOG_HOOK_FN,
    check_cert: GWEN_GUI_CHECKCERT_FN,
    progress_log: GWEN_GUI_PROGRESS_LOG_FN,
    message_box: GWEN_GUI_MESSAGEBOX_FN,
}

/// Reads the scenario the test chose from its file, `ok` when there is
/// none. Read whenever a library is loaded or initialised, since the
/// versions are asked before `GWEN_Init`.
fn load_scenario() {
    let file =
        std::env::temp_dir().join(format!("kassensturz-aqbanking-stub-{}", std::process::id()));
    let scenario = std::fs::read_to_string(file).unwrap_or_default();
    let mut state = state();
    state.scenario = if scenario.trim().is_empty() {
        "ok".to_owned()
    } else {
        scenario.trim().to_owned()
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Init() -> c_int {
    load_scenario();
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Fini() -> c_int {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Version(
    major: *mut c_int,
    minor: *mut c_int,
    patchlevel: *mut c_int,
    build: *mut c_int,
) {
    load_scenario();
    let old = state().scenario == "old-gwenhywfar";
    // SAFETY: the wrapper passes four live integers.
    unsafe {
        *major = 5;
        *minor = if old { 13 } else { 14 };
        *patchlevel = if old { 0 } else { 1 };
        *build = 0;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_GetVersion(
    major: *mut c_int,
    minor: *mut c_int,
    patchlevel: *mut c_int,
    build: *mut c_int,
) {
    // SAFETY: the wrapper passes four live integers.
    unsafe {
        *major = 6;
        *minor = 9;
        *patchlevel = 2;
        *build = 0;
    }
}

/// The two variadic functions of the bindings. Rust can't define a
/// C-variadic function on stable, so these take only the fixed
/// arguments: the symbols exist for the loader, which requires every
/// declared function, and the wrapper never calls them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Gui_ShowError(_title: *const c_char, _text: *const c_char) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Gui_ProgressLog2(
    _id: u32,
    _level: GWEN_LOGGER_LEVEL,
    _text: *const c_char,
) -> c_int {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Logger_Exists(_domain: *const c_char) -> c_int {
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Logger_SetLevel(_domain: *const c_char, _level: GWEN_LOGGER_LEVEL) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Gui_new() -> *mut GWEN_GUI {
    leak(Gui {
        get_password: None,
        set_password_status: None,
        log_hook: None,
        check_cert: None,
        progress_log: None,
        message_box: None,
    })
    .cast::<GWEN_GUI>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Gui_free(_gui: *mut GWEN_GUI) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Gui_SetGui(gui: *mut GWEN_GUI) {
    state().gui = gui.cast::<Gui>();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Gui_GetGui() -> *mut GWEN_GUI {
    state().gui.cast::<GWEN_GUI>()
}

macro_rules! setter {
    ($name:ident, $field:ident, $kind:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(gui: *mut GWEN_GUI, f: $kind) -> $kind {
            // SAFETY: the wrapper passes the GUI it got from `GWEN_Gui_new`.
            let gui = unsafe { &mut *gui.cast::<Gui>() };
            std::mem::replace(&mut gui.$field, f)
        }
    };
}

setter!(
    GWEN_Gui_SetGetPasswordFn,
    get_password,
    GWEN_GUI_GETPASSWORD_FN
);
setter!(
    GWEN_Gui_SetSetPasswordStatusFn,
    set_password_status,
    GWEN_GUI_SETPASSWORDSTATUS_FN
);
setter!(GWEN_Gui_SetLogHookFn, log_hook, GWEN_GUI_LOG_HOOK_FN);
setter!(GWEN_Gui_SetCheckCertFn, check_cert, GWEN_GUI_CHECKCERT_FN);
setter!(
    GWEN_Gui_SetProgressLogFn,
    progress_log,
    GWEN_GUI_PROGRESS_LOG_FN
);
setter!(
    GWEN_Gui_SetMessageBoxFn,
    message_box,
    GWEN_GUI_MESSAGEBOX_FN
);

fn gui() -> Option<&'static Gui> {
    let gui = state().gui;
    if gui.is_null() {
        None
    } else {
        // SAFETY: leaked, never freed.
        Some(unsafe { &*gui })
    }
}

fn log(level: GWEN_LOGGER_LEVEL, message: &str) {
    if let Some(hook) = gui().and_then(|gui| gui.log_hook) {
        let message = CString::new(message).unwrap_or_default();
        // SAFETY: calling the wrapper's own callback with a live string.
        unsafe { hook(ptr::null_mut(), c"stub".as_ptr(), level, message.as_ptr()) };
    }
}

fn progress(message: &str) {
    if let Some(hook) = gui().and_then(|gui| gui.progress_log) {
        let message = CString::new(message).unwrap_or_default();
        // SAFETY: calling the wrapper's own callback with a live string.
        unsafe {
            hook(
                ptr::null_mut(),
                0,
                GWEN_LoggerLevel_Notice,
                message.as_ptr(),
            )
        };
    }
}

/// What the bank asks for after the PIN in `scenario`, the way AqBanking
/// does: a TAN as text or as a picture, or an approval in the bank's app.
/// `0` means the job may go on.
fn second_factor(scenario: &str) -> c_int {
    match scenario {
        "tan" => ask_password(true),
        "optical-tan" => ask_tan(GWEN_Gui_PasswordMethod_OpticalHHD),
        "approval" => ask_approval(),
        _ => 0,
    }
}

/// Asks the wrapper for a TAN shown as `method`.
fn ask_tan(method: GWEN_GUI_PASSWORD_METHOD) -> c_int {
    let Some(get_password) = gui().and_then(|gui| gui.get_password) else {
        return GWEN_ERROR_NOT_IMPLEMENTED;
    };
    let mut buffer = [0 as c_char; 64];
    let token = CString::new("TAN_made-up-token").unwrap_or_default();
    // SAFETY: calling the wrapper's own callback with live strings and a
    // buffer of the length passed.
    let rv = unsafe {
        get_password(
            ptr::null_mut(),
            GWEN_GUI_INPUT_FLAGS_TAN | GWEN_GUI_INPUT_FLAGS_SHOW | GWEN_GUI_INPUT_FLAGS_DIRECT,
            token.as_ptr(),
            c"Enter TAN".as_ptr(),
            c"Scan the picture with your TAN app".as_ptr(),
            buffer.as_mut_ptr(),
            4,
            c_int::try_from(buffer.len()).unwrap_or(c_int::MAX),
            method,
            ptr::null_mut(),
            0,
        )
    };
    let length = buffer.iter().position(|&byte| byte == 0).unwrap_or(0);
    journal(&if rv < 0 {
        format!("password: optical tan declined ({rv})")
    } else {
        format!("password: optical tan with {length} characters")
    });
    rv
}

/// Asks the wrapper to have the login approved in the bank's app, the
/// way AqBanking 6.9.2 does for a decoupled TAN method
/// (`applayer/cbox_voptan.c`): a box with "Approved" and "Abort", whose
/// text is plain first and HTML after.
fn ask_approval() -> c_int {
    let Some(message_box) = gui().and_then(|gui| gui.message_box) else {
        return GWEN_ERROR_NOT_IMPLEMENTED;
    };
    // SAFETY: calling the wrapper's own callback with live strings.
    let rv = unsafe {
        message_box(
            ptr::null_mut(),
            GWEN_GUI_MSG_FLAGS_TYPE_INFO | GWEN_GUI_MSG_FLAGS_CONFIRM_B1,
            c"Decoupled Mode: Waiting for Approval".as_ptr(),
            c"Your approval is required.\nPlease use your smart phone app.\n<html><p>Your <b>approval</b> is required.</p></html>".as_ptr(),
            c"Approved".as_ptr(),
            c"Abort".as_ptr(),
            ptr::null(),
            0,
        )
    };
    journal(&format!("question: approval answered {rv}"));
    if rv == 1 { 0 } else { GWEN_ERROR_USER_ABORTED }
}

/// Asks the wrapper for the PIN (or a TAN) the way AqBanking does, and
/// plays the scenario's answer to it. `0` means the job may go on.
fn ask_password(tan: bool) -> c_int {
    let Some(gui) = gui() else {
        return GWEN_ERROR_NOT_IMPLEMENTED;
    };
    let Some(get_password) = gui.get_password else {
        return GWEN_ERROR_NOT_IMPLEMENTED;
    };
    let flags = if tan {
        GWEN_GUI_INPUT_FLAGS_TAN | GWEN_GUI_INPUT_FLAGS_SHOW
    } else {
        GWEN_GUI_INPUT_FLAGS_RETRY
    };
    let mut buffer = [0 as c_char; 64];
    let token = CString::new("PIN_made-up-token").unwrap_or_default();
    let title = CString::new(if tan { "Enter TAN" } else { "Enter PIN" }).unwrap_or_default();
    let text = CString::new(if tan {
        "Challenge: type the TAN your app shows for job 4711\n<html><p>Challenge: type the TAN</p></html>"
    } else {
        "Please enter the PIN for user made-up-login at Testbank"
    })
    .unwrap_or_default();
    // SAFETY: calling the wrapper's own callback with live strings and a
    // buffer of the length passed.
    let rv = unsafe {
        get_password(
            ptr::null_mut(),
            flags,
            token.as_ptr(),
            title.as_ptr(),
            text.as_ptr(),
            buffer.as_mut_ptr(),
            4,
            c_int::try_from(buffer.len()).unwrap_or(c_int::MAX),
            GWEN_Gui_PasswordMethod_Text,
            ptr::null_mut(),
            0,
        )
    };
    if rv < 0 {
        journal(&format!(
            "password: {} declined ({rv})",
            if tan { "tan" } else { "pin" }
        ));
        return rv;
    }
    let length = buffer.iter().position(|&byte| byte == 0).unwrap_or(0);
    journal(&format!(
        "password: {} with {length} characters",
        if tan { "tan" } else { "pin" }
    ));
    let scenario = state().scenario.clone();
    if !tan && scenario == "bad-pin" {
        if let Some(set_status) = gui.set_password_status {
            // SAFETY: the wrapper's own callback; AqBanking passes the PIN
            // it used, which the wrapper must not keep.
            unsafe {
                set_status(
                    ptr::null_mut(),
                    token.as_ptr(),
                    buffer.as_ptr(),
                    GWEN_Gui_PasswordStatus_Bad,
                    0,
                )
            };
        }
        progress("Bank says: PIN wrong (9942)");
        return GWEN_ERROR_GENERIC;
    }
    if let Some(set_status) = gui.set_password_status {
        // SAFETY: as above.
        unsafe {
            set_status(
                ptr::null_mut(),
                token.as_ptr(),
                buffer.as_ptr(),
                GWEN_Gui_PasswordStatus_Ok,
                0,
            )
        };
    }
    0
}

/// A certificate descriptor the scenario shows: a good one, or a bad one
/// in `bad-cert`.
pub struct Cert {
    is_error: c_int,
    flags: u32,
    status: *const c_char,
    name: *const c_char,
}

fn check_certificate() -> c_int {
    let Some(check) = gui().and_then(|gui| gui.check_cert) else {
        return 0;
    };
    let bad = state().scenario == "bad-cert";
    let cert = leak(Cert {
        is_error: c_int::from(bad),
        flags: if bad {
            GWEN_SSL_CERT_FLAGS_INVALID | GWEN_SSL_CERT_FLAGS_BAD_HOSTNAME
        } else {
            GWEN_SSL_CERT_FLAGS_OK
        },
        status: leak_text(if bad {
            "The certificate is invalid"
        } else {
            "The certificate is valid"
        }),
        name: leak_text("fints.testbank.example"),
    });
    // SAFETY: the wrapper's own callback with a live descriptor.
    let rv = unsafe {
        check(
            ptr::null_mut(),
            cert.cast::<GWEN_SSLCERTDESCR>(),
            ptr::null_mut(),
            0,
        )
    };
    if rv < 0 {
        journal("certificate: refused");
        return GWEN_ERROR_SSL_SECURITY;
    }
    journal("certificate: accepted");
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_SslCertDescr_GetIsError(el: *const GWEN_SSLCERTDESCR) -> c_int {
    // SAFETY: a leaked `Cert`.
    unsafe { (*el.cast::<Cert>()).is_error }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_SslCertDescr_GetStatusFlags(el: *const GWEN_SSLCERTDESCR) -> u32 {
    // SAFETY: a leaked `Cert`.
    unsafe { (*el.cast::<Cert>()).flags }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_SslCertDescr_GetStatusText(
    el: *const GWEN_SSLCERTDESCR,
) -> *const c_char {
    // SAFETY: a leaked `Cert`.
    unsafe { (*el.cast::<Cert>()).status }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_SslCertDescr_GetCommonName(
    el: *const GWEN_SSLCERTDESCR,
) -> *const c_char {
    // SAFETY: a leaked `Cert`.
    unsafe { (*el.cast::<Cert>()).name }
}

// --- Gwenhywfar: dates ----------------------------------------------------

/// A date, with the fields of `GWEN_DATE` the wrapper reads.
pub struct Date {
    year: c_int,
    month: c_int,
    day: c_int,
}

fn date(year: c_int, month: c_int, day: c_int) -> *const GWEN_DATE {
    leak(Date { year, month, day }).cast::<GWEN_DATE>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Date_fromGregorian(y: c_int, m: c_int, d: c_int) -> *mut GWEN_DATE {
    date(y, m, d).cast_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Date_free(_gd: *mut GWEN_DATE) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Date_GetYear(gd: *const GWEN_DATE) -> c_int {
    // SAFETY: a leaked `Date`.
    unsafe { (*gd.cast::<Date>()).year }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Date_GetMonth(gd: *const GWEN_DATE) -> c_int {
    // SAFETY: a leaked `Date`.
    unsafe { (*gd.cast::<Date>()).month }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn GWEN_Date_GetDay(gd: *const GWEN_DATE) -> c_int {
    // SAFETY: a leaked `Date`.
    unsafe { (*gd.cast::<Date>()).day }
}

// --- AqBanking: start-up and the setup commands ---------------------------

/// The banking object: only the folder.
pub struct Banking {
    registration_key: Option<String>,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_new(
    _app_name: *const c_char,
    dname: *const c_char,
    _extensions: u32,
) -> *mut AB_BANKING {
    // SAFETY: the wrapper passes a terminated path.
    let dir = unsafe { text(dname) }.map(PathBuf::from);
    let mut state = state();
    state.data_dir = dir;
    state.accounts.clear();
    state.next_user_id = 1;
    state.next_account_id = 1;
    drop(state);
    leak(Banking {
        registration_key: None,
    })
    .cast::<AB_BANKING>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_free(_ab: *mut AB_BANKING) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_Init(_ab: *mut AB_BANKING) -> c_int {
    let dir = state().data_dir.clone();
    if let Some(dir) = dir {
        if std::fs::create_dir_all(dir.join("settings6")).is_err() {
            return GWEN_ERROR_IO;
        }
        let _ = std::fs::remove_file(dir.join(JOURNAL_FILE));
        journal("init");
        0
    } else {
        GWEN_ERROR_INVALID
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_Fini(_ab: *mut AB_BANKING) -> c_int {
    journal("fini");
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_RuntimeConfig_SetCharValue(
    ab: *mut AB_BANKING,
    name: *const c_char,
    value: *const c_char,
) {
    // SAFETY: the wrapper passes terminated strings and its banking object.
    unsafe {
        if text(name) == Some("fintsRegistrationKey") {
            (*ab.cast::<Banking>()).registration_key = text(value).map(str::to_owned);
            journal(&format!(
                "registration key: {} characters",
                text(value).map_or(0, str::len)
            ));
        }
    }
}

fn settings_dir() -> Option<PathBuf> {
    state().data_dir.as_ref().map(|dir| dir.join("settings6"))
}

/// Writes a fake FinTS message log the way AqBanking does (D-139), so the
/// backend's clean-up has something to remove.
fn write_message_log() {
    if let Some(dir) = state().data_dir.clone() {
        let logs = dir
            .join("backends")
            .join("aqhbci")
            .join("data")
            .join("banks")
            .join("de")
            .join(BANK_CODE)
            .join("logs");
        let _ = std::fs::create_dir_all(&logs);
        let _ = std::fs::write(
            logs.join("20261023.log"),
            "HNHBK:1:3+000000000123+300+0+1'\nHKSAL for user made-up-login (PIN blanked)\n",
        );
    }
}

fn user_file(uid: u32) -> Option<PathBuf> {
    settings_dir().map(|dir| dir.join("users").join(format!("{uid:08x}.conf")))
}

fn account_file(aid: u32) -> Option<PathBuf> {
    settings_dir().map(|dir| dir.join("accounts").join(format!("{aid:08x}.conf")))
}

fn option<'a>(arguments: &[&'a str], short: &str, long: &str) -> Option<&'a str> {
    let mut iterator = arguments.iter();
    while let Some(argument) = iterator.next() {
        if *argument == short || *argument == long {
            return iterator.next().copied();
        }
        if let Some(value) = argument
            .strip_prefix(long)
            .and_then(|rest| rest.strip_prefix('='))
        {
            return Some(value);
        }
    }
    None
}

fn add_user(arguments: &[&str]) -> c_int {
    let (Some(bank), Some(login), Some(server), Some(kind), Some(name)) = (
        option(arguments, "-b", "--bank"),
        option(arguments, "-u", "--user"),
        option(arguments, "-s", "--server"),
        option(arguments, "-t", "--tokentype"),
        option(arguments, "-N", "--username"),
    ) else {
        eprintln!("stub: adduser needs -b, -u, -s, -t and -N");
        return 1;
    };
    if kind != "pintan" {
        return 3;
    }
    let customer = option(arguments, "-c", "--customer").unwrap_or(login);
    let uid = {
        let mut state = state();
        let uid = state.next_user_id;
        state.next_user_id += 1;
        uid
    };
    let Some(file) = user_file(uid) else {
        return GWEN_ERROR_INVALID;
    };
    let _ = std::fs::create_dir_all(file.parent().unwrap_or(&file));
    let content = format!(
        "int uniqueId=\"{uid}\"\nchar backendName=\"aqhbci\"\nchar userName=\"{name}\"\n\
         char userId=\"{login}\"\nchar customerId=\"{customer}\"\nchar country=\"de\"\n\
         char bankCode=\"{bank}\"\nint lastSessionId=\"0\"\ndata {{\n  backend {{\n    \
         char cryptMode=\"pintan\"\n    char server=\"{}\"\n    int hbciVersion=\"300\"\n  }}\n}}\n",
        server.replace(':', "%3A").replace('/', "%2F")
    );
    if std::fs::write(&file, content).is_err() {
        return GWEN_ERROR_IO;
    }
    0
}

fn user_option(arguments: &[&str]) -> Option<u32> {
    option(arguments, "-u", "--user").and_then(|value| value.parse().ok())
}

fn has_user(uid: u32) -> bool {
    user_file(uid).is_some_and(|file| file.exists())
}

fn get_accounts(uid: u32) -> c_int {
    let rv = ask_password(false);
    if rv != 0 {
        return rv;
    }
    write_message_log();
    let mut state = state();
    if state.accounts.iter().any(|account| account.user_id == uid) {
        return 0;
    }
    for (kind, iban, number, name) in [
        (
            AB_AccountType_Checking,
            Some(CURRENT_IBAN),
            CURRENT_NUMBER,
            "Girokonto",
        ),
        (AB_AccountType_CreditCard, None, CARD_NUMBER, "Kreditkarte"),
    ] {
        let aid = state.next_account_id;
        state.next_account_id += 1;
        state.accounts.push(Account {
            unique_id: aid,
            user_id: uid,
            kind,
            iban,
            number,
            name,
        });
        drop(state);
        if let Some(file) = account_file(aid) {
            let _ = std::fs::create_dir_all(file.parent().unwrap_or(&file));
            let _ = std::fs::write(
                file,
                format!(
                    "int uniqueId=\"{aid}\"\nint userId=\"{uid}\"\nchar backendName=\"aqhbci\"\n\
                     char bankCode=\"{BANK_CODE}\"\nchar accountNumber=\"{number}\"\n"
                ),
            );
        }
        state = self::state();
    }
    0
}

fn delete_user(uid: u32) -> c_int {
    if !has_user(uid) {
        return 2;
    }
    if let Some(file) = user_file(uid) {
        let _ = std::fs::remove_file(file);
    }
    let removed: Vec<u32> = {
        let mut state = state();
        let removed = state
            .accounts
            .iter()
            .filter(|account| account.user_id == uid)
            .map(|account| account.unique_id)
            .collect();
        state.accounts.retain(|account| account.user_id != uid);
        removed
    };
    for aid in removed {
        if let Some(file) = account_file(aid) {
            let _ = std::fs::remove_file(file);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_ProviderControl(
    _ab: *mut AB_BANKING,
    backend: *const c_char,
    argc: c_int,
    argv: *mut *mut c_char,
) -> c_int {
    // SAFETY: the wrapper passes `argc` terminated strings.
    let arguments: Vec<&str> = unsafe {
        if text(backend) != Some("aqhbci") {
            return GWEN_ERROR_NOT_FOUND;
        }
        (0..usize::try_from(argc).unwrap_or(0))
            .filter_map(|index| text(*argv.add(index)))
            .collect()
    };
    journal(&format!("control: {}", arguments.join(" ")));
    // A test may switch the scenario between calls, e.g. after the setup.
    load_scenario();
    let Some((&command, arguments)) = arguments.split_first() else {
        return 1;
    };
    if state().scenario == "unreachable" && command != "adduser" && command != "deluser" {
        progress("Could not connect to server");
        return GWEN_ERROR_NET_UNREACHABLE;
    }
    match command {
        "adduser" => add_user(arguments),
        "deluser" => user_option(arguments).map_or(1, delete_user),
        "getbankinfo" | "getaccsepa" => {
            if user_option(arguments).is_some_and(has_user) {
                0
            } else {
                1
            }
        }
        "getsysid" => {
            if !user_option(arguments).is_some_and(has_user) {
                return 1;
            }
            let rv = check_certificate();
            if rv != 0 {
                return rv;
            }
            let rv = ask_password(false);
            if rv != 0 {
                return rv;
            }
            let scenario = state().scenario.clone();
            let rv = second_factor(scenario.strip_prefix("setup-").unwrap_or_default());
            if rv != 0 {
                return rv;
            }
            write_message_log();
            0
        }
        "getaccounts" => match user_option(arguments) {
            Some(uid) if has_user(uid) => get_accounts(uid),
            _ => 1,
        },
        _ => {
            eprintln!("stub: unknown command {command}");
            1
        }
    }
}

// --- AqBanking: bank information ------------------------------------------

/// A bank information record with its services.
pub struct BankInfo {
    code: *const c_char,
    name: *const c_char,
    bic: *const c_char,
    services: *mut Service,
}

/// One service, as a linked list element.
pub struct Service {
    kind: *const c_char,
    address: *const c_char,
    mode: *const c_char,
    version: *const c_char,
    next: *mut Service,
}

/// A FinTS service of a stub bank.
fn service(address: &str, mode: &str, next: *mut Service) -> *mut Service {
    leak(Service {
        kind: leak_text("HBCI"),
        address: leak_text(address),
        mode: leak_text(mode),
        version: leak_text("3.0"),
        next,
    })
}

/// A PIN/TAN service alone.
fn pin_tan(address: &str) -> *mut Service {
    service(address, "PINTAN", ptr::null_mut())
}

fn record(code: &str, name: &str, bic: Option<&str>, services: *mut Service) -> BankInfo {
    BankInfo {
        code: leak_text(code),
        name: leak_text(name),
        bic: bic.map_or(ptr::null(), leak_text),
        services,
    }
}

/// The server of the two Atruvia hosts the cooperative stub banks use.
const ATRUVIA_NORTH: &str = "https://fints1.atruvia.de/cgi-bin/hbciservlet";
const ATRUVIA_SOUTH: &str = "https://fints2.atruvia.de/cgi-bin/hbciservlet";

/// Every record of the stub's bank information, in the order of the data.
/// A code can have several records, and only the first is what
/// `AB_Banking_GetBankInfo` returns, as in AqBanking's data.
fn records() -> Vec<BankInfo> {
    vec![
        record(
            BANK_CODE,
            "Testbank",
            Some("COBADEFFXXX"),
            service(
                "https://fints.testbank.example/",
                "PINTAN",
                service("https://fints.testbank.example/", "RDH10", ptr::null_mut()),
            ),
        ),
        // A bank without any service, and one whose service has no address.
        record(
            "10000000",
            "Bundesbank",
            Some("MARKDEF1100"),
            ptr::null_mut(),
        ),
        record(
            "20000000",
            "Bank ohne Adresse",
            None,
            leak(Service {
                kind: leak_text("HBCI"),
                address: ptr::null(),
                mode: ptr::null(),
                version: ptr::null(),
                next: ptr::null_mut(),
            }),
        ),
        // Text that isn't UTF-8 in a bank's name (T-09).
        BankInfo {
            code: leak_text("30000000"),
            name: leak_bytes(b"Sparkasse K\xf6ln"),
            bic: ptr::null(),
            services: pin_tan("https://fints.koeln.example/"),
        },
        // Cooperative banks of one region: two on the southern Atruvia
        // server, one with no service at all, and one whose first record
        // has no service while a later record of the same code has.
        record(
            "55500001",
            "Genossenbank Testhausen",
            Some("GENODEF1TS1"),
            pin_tan(ATRUVIA_SOUTH),
        ),
        record(
            "55500002",
            "Genossenbank Beispielstadt",
            Some("GENODEF1BS1"),
            pin_tan(ATRUVIA_SOUTH),
        ),
        record(
            "55500003",
            "Genossenbank Muster",
            Some("GENODEF1MU1"),
            ptr::null_mut(),
        ),
        // The first record has no service and no BIC; the second has both.
        record("55600001", "Genossenbank Spaet", None, ptr::null_mut()),
        record(
            "55600001",
            "Genossenbank Spaet (Geschaeftsstelle)",
            Some("GENODEF1SP1"),
            pin_tan("https://fints.spaet.example/"),
        ),
        // A cooperative bank next to a Sparda bank on a northern server.
        record(
            "77700001",
            "Sparda-Bank Teststadt",
            Some("GENODEF1S99"),
            ptr::null_mut(),
        ),
        record(
            "77700002",
            "Genossenbank Nachbar",
            Some("GENODEF1NB1"),
            pin_tan(ATRUVIA_NORTH),
        ),
        // A cooperative bank with no neighbour at all.
        record(
            "88800001",
            "Genossenbank Allein",
            Some("GENODEF1AL1"),
            ptr::null_mut(),
        ),
        // A bank that isn't cooperative next to one that is.
        record(
            "88900001",
            "Testbank Anders",
            Some("TESTDEFFXXX"),
            ptr::null_mut(),
        ),
        record(
            "88900002",
            "Genossenbank Daneben",
            Some("GENODEF1DN1"),
            pin_tan(ATRUVIA_SOUTH),
        ),
        // Neighbours that aren't on an Atruvia server, and two that are
        // split between the two hosts.
        record(
            "99900001",
            "Genossenbank Eigenrechner",
            Some("GENODEF1EG1"),
            ptr::null_mut(),
        ),
        record(
            "99900002",
            "Genossenbank Fremd",
            Some("GENODEF1FR1"),
            pin_tan("https://fints.fremd.example/"),
        ),
        record(
            "99800001",
            "Genossenbank Unentschieden",
            Some("GENODEF1UE1"),
            ptr::null_mut(),
        ),
        record(
            "99800002",
            "Genossenbank Norden",
            Some("GENODEF1NO1"),
            pin_tan(ATRUVIA_NORTH),
        ),
        record(
            "99800003",
            "Genossenbank Sueden",
            Some("GENODEF1SU1"),
            pin_tan(ATRUVIA_SOUTH),
        ),
    ]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_GetBankInfo(
    _ab: *mut AB_BANKING,
    _country: *const c_char,
    _branch: *const c_char,
    bank_id: *const c_char,
) -> *mut AB_BANKINFO {
    // SAFETY: the wrapper passes a terminated string.
    let Some(code) = (unsafe { text(bank_id) }) else {
        return ptr::null_mut();
    };
    records()
        .into_iter()
        // SAFETY: the code of a record the stub made.
        .find(|info| unsafe { text(info.code) } == Some(code))
        .map_or(ptr::null_mut(), |info| leak(info).cast::<AB_BANKINFO>())
}

/// A list of bank information records, with the position of its iterator.
pub struct BankList {
    items: Vec<*mut BankInfo>,
}

/// An iterator over a [`BankList`].
pub struct BankListIterator {
    list: *const BankList,
    position: usize,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_new() -> *mut AB_BANKINFO {
    leak(record("", "", None, ptr::null_mut())).cast::<AB_BANKINFO>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_SetBankId(info: *mut AB_BANKINFO, code: *const c_char) {
    // SAFETY: a leaked `BankInfo`; the wrapper passes a terminated string.
    unsafe {
        (*info.cast::<BankInfo>()).code = code;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_List2_new() -> *mut AB_BANKINFO_LIST2 {
    leak(BankList { items: Vec::new() }).cast::<AB_BANKINFO_LIST2>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_List2_freeAll(_list: *mut AB_BANKINFO_LIST2) -> c_int {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_List2_First(
    list: *const AB_BANKINFO_LIST2,
) -> *mut AB_BANKINFO_LIST2_ITERATOR {
    // SAFETY: a leaked `BankList`.
    if unsafe { (*list.cast::<BankList>()).items.is_empty() } {
        return ptr::null_mut();
    }
    leak(BankListIterator {
        list: list.cast::<BankList>(),
        position: 0,
    })
    .cast::<AB_BANKINFO_LIST2_ITERATOR>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_List2Iterator_Data(
    iterator: *mut AB_BANKINFO_LIST2_ITERATOR,
) -> *mut AB_BANKINFO {
    // SAFETY: a leaked iterator over a leaked list.
    unsafe {
        let iterator = &*iterator.cast::<BankListIterator>();
        let list = &*iterator.list;
        list.items
            .get(iterator.position)
            .map_or(ptr::null_mut(), |info| info.cast::<AB_BANKINFO>())
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_List2Iterator_Next(
    iterator: *mut AB_BANKINFO_LIST2_ITERATOR,
) -> *mut AB_BANKINFO {
    // SAFETY: a leaked iterator over a leaked list.
    unsafe {
        let iterator = &mut *iterator.cast::<BankListIterator>();
        iterator.position += 1;
        let list = &*iterator.list;
        list.items
            .get(iterator.position)
            .map_or(ptr::null_mut(), |info| info.cast::<AB_BANKINFO>())
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_List2Iterator_free(
    _iterator: *mut AB_BANKINFO_LIST2_ITERATOR,
) {
}

/// Like AqBanking's search by template: the template's code, where a `*`
/// at the end stands for any further digits, picks the records; the
/// others are not looked at.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_GetBankInfoByTemplate(
    _ab: *mut AB_BANKING,
    _country: *const c_char,
    template: *mut AB_BANKINFO,
    list: *mut AB_BANKINFO_LIST2,
) -> c_int {
    // SAFETY: leaked objects of the stub's own types.
    unsafe {
        let Some(pattern) = text((*template.cast::<BankInfo>()).code) else {
            return -1;
        };
        let list = &mut *list.cast::<BankList>();
        for info in records() {
            let code = text(info.code).unwrap_or_default();
            let matches = match pattern.strip_suffix('*') {
                Some(prefix) => code.starts_with(prefix),
                None => code == pattern,
            };
            if matches {
                list.items.push(leak(info));
            }
        }
        if list.items.is_empty() { -1 } else { 0 }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_free(_info: *mut AB_BANKINFO) {}

macro_rules! getter {
    ($name:ident, $object:ty, $kind:ty, $field:ident, $out:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(object: *const $object) -> $out {
            // SAFETY: a leaked object of the stub's own type.
            unsafe { (*object.cast::<$kind>()).$field }
        }
    };
}

/// A getter for a list element's `next` pointer, cast to the C type.
macro_rules! next_getter {
    ($name:ident, $object:ty, $kind:ty) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(object: *const $object) -> *mut $object {
            // SAFETY: a leaked object of the stub's own type.
            unsafe { (*object.cast::<$kind>()).next.cast::<$object>() }
        }
    };
}

getter!(
    AB_BankInfo_GetBankId,
    AB_BANKINFO,
    BankInfo,
    code,
    *const c_char
);
getter!(
    AB_BankInfo_GetBankName,
    AB_BANKINFO,
    BankInfo,
    name,
    *const c_char
);
getter!(
    AB_BankInfo_GetBic,
    AB_BANKINFO,
    BankInfo,
    bic,
    *const c_char
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfo_GetServices(
    info: *const AB_BANKINFO,
) -> *mut AB_BANKINFO_SERVICE_LIST {
    // The list is its first element.
    // SAFETY: a leaked `BankInfo`.
    unsafe {
        (*info.cast::<BankInfo>())
            .services
            .cast::<AB_BANKINFO_SERVICE_LIST>()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_BankInfoService_List_First(
    list: *const AB_BANKINFO_SERVICE_LIST,
) -> *mut AB_BANKINFO_SERVICE {
    list.cast_mut().cast::<AB_BANKINFO_SERVICE>()
}

next_getter!(AB_BankInfoService_List_Next, AB_BANKINFO_SERVICE, Service);
getter!(
    AB_BankInfoService_GetType,
    AB_BANKINFO_SERVICE,
    Service,
    kind,
    *const c_char
);
getter!(
    AB_BankInfoService_GetAddress,
    AB_BANKINFO_SERVICE,
    Service,
    address,
    *const c_char
);
getter!(
    AB_BankInfoService_GetMode,
    AB_BANKINFO_SERVICE,
    Service,
    mode,
    *const c_char
);
getter!(
    AB_BankInfoService_GetPversion,
    AB_BANKINFO_SERVICE,
    Service,
    version,
    *const c_char
);

// --- AqBanking: account specifications ------------------------------------

/// An account specification, as a linked list element.
pub struct Spec {
    unique_id: u32,
    kind: c_int,
    iban: *const c_char,
    number: *const c_char,
    name: *const c_char,
    currency: *const c_char,
    next: *mut Spec,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_GetAccountSpecList(
    _ab: *const AB_BANKING,
    list: *mut *mut AB_ACCOUNT_SPEC_LIST,
) -> c_int {
    let state = state();
    let mut first: *mut Spec = ptr::null_mut();
    for account in state.accounts.iter().rev() {
        first = leak(Spec {
            unique_id: account.unique_id,
            kind: account.kind,
            iban: account.iban.map_or(ptr::null(), leak_text),
            number: leak_text(account.number),
            name: leak_text(account.name),
            currency: leak_text("EUR"),
            next: first,
        });
    }
    if first.is_null() {
        return GWEN_ERROR_NOT_FOUND;
    }
    // SAFETY: the wrapper passes a live out-pointer.
    unsafe { *list = first.cast::<AB_ACCOUNT_SPEC_LIST>() };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_AccountSpec_List_First(
    list: *const AB_ACCOUNT_SPEC_LIST,
) -> *mut AB_ACCOUNT_SPEC {
    list.cast_mut().cast::<AB_ACCOUNT_SPEC>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_AccountSpec_List_free(_list: *mut AB_ACCOUNT_SPEC_LIST) {}

next_getter!(AB_AccountSpec_List_Next, AB_ACCOUNT_SPEC, Spec);
getter!(
    AB_AccountSpec_GetUniqueId,
    AB_ACCOUNT_SPEC,
    Spec,
    unique_id,
    u32
);
getter!(AB_AccountSpec_GetType, AB_ACCOUNT_SPEC, Spec, kind, c_int);
getter!(
    AB_AccountSpec_GetIban,
    AB_ACCOUNT_SPEC,
    Spec,
    iban,
    *const c_char
);
getter!(
    AB_AccountSpec_GetAccountNumber,
    AB_ACCOUNT_SPEC,
    Spec,
    number,
    *const c_char
);
getter!(
    AB_AccountSpec_GetAccountName,
    AB_ACCOUNT_SPEC,
    Spec,
    name,
    *const c_char
);
getter!(
    AB_AccountSpec_GetCurrency,
    AB_ACCOUNT_SPEC,
    Spec,
    currency,
    *const c_char
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_AccountSpec_GetBankCode(
    _spec: *const AB_ACCOUNT_SPEC,
) -> *const c_char {
    leak_text(BANK_CODE)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_AccountSpec_GetCountry(_spec: *const AB_ACCOUNT_SPEC) -> *const c_char {
    leak_text("de")
}

// --- AqBanking: the fetch -------------------------------------------------

/// A command the wrapper builds.
pub struct Command {
    command: AB_TRANSACTION_COMMAND,
    account: u32,
    first_date: Option<(c_int, c_int, c_int)>,
}

/// The command list: just the commands.
pub struct Commands {
    commands: Vec<*mut Command>,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Transaction_new() -> *mut AB_TRANSACTION {
    leak(Command {
        command: AB_Transaction_CommandNone,
        account: 0,
        first_date: None,
    })
    .cast::<AB_TRANSACTION>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Transaction_SetCommand(
    transaction: *mut AB_TRANSACTION,
    command: AB_TRANSACTION_COMMAND,
) {
    // SAFETY: a leaked `Command`.
    unsafe { (*transaction.cast::<Command>()).command = command };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Transaction_SetUniqueAccountId(
    transaction: *mut AB_TRANSACTION,
    id: u32,
) {
    // SAFETY: a leaked `Command`.
    unsafe { (*transaction.cast::<Command>()).account = id };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Transaction_SetFirstDate(
    transaction: *mut AB_TRANSACTION,
    date: *const GWEN_DATE,
) {
    // SAFETY: a leaked `Command` and a leaked `Date`.
    unsafe {
        let date = &*date.cast::<Date>();
        (*transaction.cast::<Command>()).first_date = Some((date.year, date.month, date.day));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Transaction_List2_new() -> *mut AB_TRANSACTION_LIST2 {
    leak(Commands {
        commands: Vec::new(),
    })
    .cast::<AB_TRANSACTION_LIST2>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Transaction_List2_PushBack(
    list: *mut AB_TRANSACTION_LIST2,
    transaction: *mut AB_TRANSACTION,
) {
    // SAFETY: a leaked `Commands`.
    unsafe {
        (*list.cast::<Commands>())
            .commands
            .push(transaction.cast::<Command>())
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Transaction_List2_freeAll(_list: *mut AB_TRANSACTION_LIST2) -> c_int {
    0
}

/// An amount with the fields of `AB_VALUE` the wrapper reads.
pub struct Value {
    fraction: &'static str,
    currency: *const c_char,
}

fn value(fraction: &'static str, currency: Option<&str>) -> *const AB_VALUE {
    leak(Value {
        fraction,
        currency: currency.map_or(ptr::null(), leak_text),
    })
    .cast::<AB_VALUE>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Value_GetNumDenomString(
    value: *const AB_VALUE,
    buffer: *mut c_char,
    buflen: u32,
) -> c_int {
    // SAFETY: a leaked `Value` and a buffer of `buflen` bytes.
    unsafe {
        let fraction = (*value.cast::<Value>()).fraction.as_bytes();
        let Ok(buflen) = usize::try_from(buflen) else {
            return GWEN_ERROR_BUFFER_OVERFLOW;
        };
        if fraction.len() + 1 > buflen {
            return GWEN_ERROR_BUFFER_OVERFLOW;
        }
        ptr::copy_nonoverlapping(fraction.as_ptr().cast::<c_char>(), buffer, fraction.len());
        buffer.add(fraction.len()).write(0);
    }
    0
}

getter!(
    AB_Value_GetCurrency,
    AB_VALUE,
    Value,
    currency,
    *const c_char
);

/// A balance, as a linked list element.
pub struct Balance {
    kind: AB_BALANCE_TYPE,
    date: *const GWEN_DATE,
    value: *const AB_VALUE,
    next: *mut Balance,
}

/// A fetched transaction, as a linked list element.
pub struct Transaction {
    kind: AB_TRANSACTION_TYPE,
    fi_id: *const c_char,
    date: *const GWEN_DATE,
    valuta: *const GWEN_DATE,
    value: *const AB_VALUE,
    remote_name: *const c_char,
    remote_iban: *const c_char,
    purpose: *const c_char,
    next: *mut Transaction,
}

/// What the bank answered for one account, as a linked list element.
pub struct AccountInfo {
    account_id: u32,
    iban: *const c_char,
    number: *const c_char,
    bank_code: *const c_char,
    currency: *const c_char,
    balances: *mut Balance,
    transactions: *mut Transaction,
    next: *mut AccountInfo,
}

/// A message from the bank, as a linked list element.
pub struct Message {
    text: *const c_char,
    next: *mut Message,
}

/// The context the fetch fills.
pub struct Context {
    accounts: *mut AccountInfo,
    messages: *mut Message,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_ImExporterContext_new() -> *mut AB_IMEXPORTER_CONTEXT {
    leak(Context {
        accounts: ptr::null_mut(),
        messages: ptr::null_mut(),
    })
    .cast::<AB_IMEXPORTER_CONTEXT>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_ImExporterContext_free(_context: *mut AB_IMEXPORTER_CONTEXT) {}

fn add_message(context: &mut Context, text: &str) {
    context.messages = leak(Message {
        text: leak_text(text),
        next: context.messages,
    });
}

/// One transaction of a scenario, field by field as the bindings read
/// them; a struct of options would only move the same ten values.
#[allow(clippy::too_many_arguments)]
fn transaction(
    kind: AB_TRANSACTION_TYPE,
    fi_id: Option<&str>,
    booked: Option<(c_int, c_int, c_int)>,
    valuta: Option<(c_int, c_int, c_int)>,
    fraction: &'static str,
    currency: Option<&str>,
    remote_name: *const c_char,
    remote_iban: Option<&str>,
    purpose: *const c_char,
    next: *mut Transaction,
) -> *mut Transaction {
    leak(Transaction {
        kind,
        fi_id: fi_id.map_or(ptr::null(), leak_text),
        date: booked.map_or(ptr::null(), |(y, m, d)| date(y, m, d)),
        valuta: valuta.map_or(ptr::null(), |(y, m, d)| date(y, m, d)),
        value: value(fraction, currency),
        remote_name,
        remote_iban: remote_iban.map_or(ptr::null(), leak_text),
        purpose,
        next,
    })
}

/// The transactions the scenario answers with, newest first in the list
/// as AqBanking delivers them (the bank's order), filtered by the
/// command's first date.
fn transactions(scenario: &str, since: Option<(c_int, c_int, c_int)>) -> *mut Transaction {
    let mut list: Vec<*mut Transaction> = Vec::new();
    match scenario {
        "broken-amount" => list.push(transaction(
            AB_Transaction_TypeStatement,
            Some("T-9"),
            Some((2026, 10, 15)),
            Some((2026, 10, 15)),
            "abc",
            Some("EUR"),
            leak_text("Someone"),
            None,
            leak_text("broken"),
            ptr::null_mut(),
        )),
        "broken-inexact" => list.push(transaction(
            AB_Transaction_TypeStatement,
            Some("T-9"),
            Some((2026, 10, 15)),
            Some((2026, 10, 15)),
            "1/3",
            Some("EUR"),
            leak_text("Someone"),
            None,
            leak_text("a third of a euro"),
            ptr::null_mut(),
        )),
        "broken-date" => list.push(transaction(
            AB_Transaction_TypeStatement,
            Some("T-9"),
            None,
            None,
            "-100/100",
            Some("EUR"),
            leak_text("Someone"),
            None,
            leak_text("no date"),
            ptr::null_mut(),
        )),
        "broken-currency" => list.push(transaction(
            AB_Transaction_TypeStatement,
            Some("T-9"),
            Some((2026, 10, 15)),
            None,
            "-100/100",
            Some("USD"),
            leak_text("Someone"),
            None,
            leak_text("dollars"),
            ptr::null_mut(),
        )),
        "broken-text" => {
            // Text that isn't UTF-8, a name longer than any limit, and no
            // ID, no currency (the account's counts), no value date.
            let long = "N".repeat(kassensturz_aqbanking_sys::MAX_TEXT_BYTES + 500);
            list.push(transaction(
                AB_Transaction_TypeStatement,
                None,
                Some((2026, 10, 15)),
                None,
                "-1999/100",
                None,
                leak_text(&long),
                None,
                leak_bytes(b"Caf\xe9 am Markt\nTisch 4"),
                ptr::null_mut(),
            ));
        }
        _ => {
            let all = [
                (
                    AB_Transaction_TypeStatement,
                    Some("T-1"),
                    (2026, 10, 15),
                    "-4999/100",
                    "Supermarkt Nord",
                    "Einkauf\nKarte 1234",
                ),
                (
                    AB_Transaction_TypeStatement,
                    Some("T-2"),
                    (2026, 10, 1),
                    "-85000/100",
                    "Vermieter",
                    "Miete Oktober",
                ),
                (
                    AB_Transaction_TypeNotedStatement,
                    None,
                    (2026, 10, 21),
                    "250000/100",
                    "Arbeitgeber GmbH",
                    "Gehalt",
                ),
            ];
            for (kind, fi_id, booked, fraction, name, purpose) in all {
                if since.is_some_and(|since| booked < since) {
                    continue;
                }
                list.push(transaction(
                    kind,
                    fi_id,
                    Some(booked),
                    Some(booked),
                    fraction,
                    Some("EUR"),
                    leak_text(name),
                    Some(OTHER_IBAN),
                    leak_text(purpose),
                    ptr::null_mut(),
                ));
            }
        }
    }
    let mut first: *mut Transaction = ptr::null_mut();
    for item in list.into_iter().rev() {
        // SAFETY: leaked, never freed.
        unsafe { (*item).next = first };
        first = item;
    }
    first
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Banking_SendCommands(
    _ab: *mut AB_BANKING,
    commands: *mut AB_TRANSACTION_LIST2,
    context: *mut AB_IMEXPORTER_CONTEXT,
) -> c_int {
    // SAFETY: leaked objects the wrapper built.
    let (commands, context) = unsafe {
        (
            &*commands.cast::<Commands>(),
            &mut *context.cast::<Context>(),
        )
    };
    let (mut account, mut since) = (None, None);
    for &command in &commands.commands {
        // SAFETY: a leaked `Command`.
        let command = unsafe { &*command };
        account = Some(command.account);
        if command.command == AB_Transaction_CommandGetTransactions {
            since = command.first_date;
        }
    }
    let Some(account) = account else {
        return GWEN_ERROR_BAD_DATA;
    };
    journal(&format!(
        "fetch: account {account} since {}",
        since.map_or("-".to_owned(), |(y, m, d)| format!("{y:04}-{m:02}-{d:02}"))
    ));
    load_scenario();
    let scenario = state().scenario.clone();
    match scenario.as_str() {
        "unreachable" => {
            progress("Could not connect to fints.testbank.example");
            return GWEN_ERROR_NET_UNREACHABLE;
        }
        "refused" => {
            add_message(context, "Auftrag abgelehnt: Konto gesperrt (9010)");
            return GWEN_ERROR_GENERIC;
        }
        _ => {}
    }
    log(
        GWEN_LoggerLevel_Warning,
        "stub: connecting to the made-up bank",
    );
    let rv = check_certificate();
    if rv != 0 {
        return rv;
    }
    let rv = ask_password(false);
    if rv != 0 {
        return rv;
    }
    let rv = second_factor(&scenario);
    if rv != 0 {
        return rv;
    }
    write_message_log();
    let known = state()
        .accounts
        .iter()
        .any(|known| known.unique_id == account);
    if !known || scenario == "empty" {
        add_message(context, "No such account");
        return 0;
    }
    let (iban, number) = {
        let state = state();
        let found = state
            .accounts
            .iter()
            .find(|known| known.unique_id == account);
        (
            found
                .and_then(|found| found.iban)
                .map_or(ptr::null(), leak_text),
            found.map_or(ptr::null(), |found| leak_text(found.number)),
        )
    };
    let balances = if scenario == "broken-balance" {
        leak(Balance {
            kind: AB_Balance_TypeBooked,
            date: date(2026, 10, 23),
            value: value("12345/1000", Some("EUR")),
            next: ptr::null_mut(),
        })
    } else {
        leak(Balance {
            kind: AB_Balance_TypeNoted,
            date: date(2026, 10, 23),
            value: value("373457/100", Some("EUR")),
            next: leak(Balance {
                kind: AB_Balance_TypeBooked,
                date: date(2026, 10, 22),
                value: value("123457/100", Some("EUR")),
                next: ptr::null_mut(),
            }),
        })
    };
    context.accounts = leak(AccountInfo {
        account_id: account,
        iban,
        number,
        bank_code: leak_text(BANK_CODE),
        currency: if scenario == "broken-text" {
            ptr::null()
        } else {
            leak_text("EUR")
        },
        balances,
        transactions: transactions(&scenario, since),
        next: ptr::null_mut(),
    });
    add_message(context, "Hinweis: Auftrag ausgeführt");
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_ImExporterContext_GetAccountInfoList(
    context: *const AB_IMEXPORTER_CONTEXT,
) -> *mut AB_IMEXPORTER_ACCOUNTINFO_LIST {
    // SAFETY: a leaked `Context`.
    unsafe {
        (*context.cast::<Context>())
            .accounts
            .cast::<AB_IMEXPORTER_ACCOUNTINFO_LIST>()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_ImExporterContext_GetMessageList(
    context: *const AB_IMEXPORTER_CONTEXT,
) -> *mut AB_MESSAGE_LIST {
    // SAFETY: a leaked `Context`.
    unsafe {
        (*context.cast::<Context>())
            .messages
            .cast::<AB_MESSAGE_LIST>()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_ImExporterAccountInfo_List_First(
    list: *const AB_IMEXPORTER_ACCOUNTINFO_LIST,
) -> *mut AB_IMEXPORTER_ACCOUNTINFO {
    list.cast_mut().cast::<AB_IMEXPORTER_ACCOUNTINFO>()
}

next_getter!(
    AB_ImExporterAccountInfo_List_Next,
    AB_IMEXPORTER_ACCOUNTINFO,
    AccountInfo
);
getter!(
    AB_ImExporterAccountInfo_GetAccountId,
    AB_IMEXPORTER_ACCOUNTINFO,
    AccountInfo,
    account_id,
    u32
);
getter!(
    AB_ImExporterAccountInfo_GetIban,
    AB_IMEXPORTER_ACCOUNTINFO,
    AccountInfo,
    iban,
    *const c_char
);
getter!(
    AB_ImExporterAccountInfo_GetAccountNumber,
    AB_IMEXPORTER_ACCOUNTINFO,
    AccountInfo,
    number,
    *const c_char
);
getter!(
    AB_ImExporterAccountInfo_GetBankCode,
    AB_IMEXPORTER_ACCOUNTINFO,
    AccountInfo,
    bank_code,
    *const c_char
);
getter!(
    AB_ImExporterAccountInfo_GetCurrency,
    AB_IMEXPORTER_ACCOUNTINFO,
    AccountInfo,
    currency,
    *const c_char
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_ImExporterAccountInfo_GetBalanceList(
    info: *const AB_IMEXPORTER_ACCOUNTINFO,
) -> *mut AB_BALANCE_LIST {
    // SAFETY: a leaked `AccountInfo`.
    unsafe {
        (*info.cast::<AccountInfo>())
            .balances
            .cast::<AB_BALANCE_LIST>()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_ImExporterAccountInfo_GetTransactionList(
    info: *const AB_IMEXPORTER_ACCOUNTINFO,
) -> *mut AB_TRANSACTION_LIST {
    // SAFETY: a leaked `AccountInfo`.
    unsafe {
        (*info.cast::<AccountInfo>())
            .transactions
            .cast::<AB_TRANSACTION_LIST>()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Balance_List_First(list: *const AB_BALANCE_LIST) -> *mut AB_BALANCE {
    list.cast_mut().cast::<AB_BALANCE>()
}

next_getter!(AB_Balance_List_Next, AB_BALANCE, Balance);
getter!(
    AB_Balance_GetType,
    AB_BALANCE,
    Balance,
    kind,
    AB_BALANCE_TYPE
);
getter!(
    AB_Balance_GetDate,
    AB_BALANCE,
    Balance,
    date,
    *const GWEN_DATE
);
getter!(
    AB_Balance_GetValue,
    AB_BALANCE,
    Balance,
    value,
    *const AB_VALUE
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Transaction_List_First(
    list: *const AB_TRANSACTION_LIST,
) -> *mut AB_TRANSACTION {
    list.cast_mut().cast::<AB_TRANSACTION>()
}

next_getter!(AB_Transaction_List_Next, AB_TRANSACTION, Transaction);
getter!(
    AB_Transaction_GetType,
    AB_TRANSACTION,
    Transaction,
    kind,
    AB_TRANSACTION_TYPE
);
getter!(
    AB_Transaction_GetFiId,
    AB_TRANSACTION,
    Transaction,
    fi_id,
    *const c_char
);
getter!(
    AB_Transaction_GetDate,
    AB_TRANSACTION,
    Transaction,
    date,
    *const GWEN_DATE
);
getter!(
    AB_Transaction_GetValutaDate,
    AB_TRANSACTION,
    Transaction,
    valuta,
    *const GWEN_DATE
);
getter!(
    AB_Transaction_GetValue,
    AB_TRANSACTION,
    Transaction,
    value,
    *const AB_VALUE
);
getter!(
    AB_Transaction_GetRemoteName,
    AB_TRANSACTION,
    Transaction,
    remote_name,
    *const c_char
);
getter!(
    AB_Transaction_GetRemoteIban,
    AB_TRANSACTION,
    Transaction,
    remote_iban,
    *const c_char
);
getter!(
    AB_Transaction_GetPurpose,
    AB_TRANSACTION,
    Transaction,
    purpose,
    *const c_char
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn AB_Message_List_First(list: *const AB_MESSAGE_LIST) -> *mut AB_MESSAGE {
    list.cast_mut().cast::<AB_MESSAGE>()
}

next_getter!(AB_Message_List_Next, AB_MESSAGE, Message);
getter!(AB_Message_GetText, AB_MESSAGE, Message, text, *const c_char);
