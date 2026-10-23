// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Raw declarations of the AqBanking and Gwenhywfar C functions that
//! KassenSturz reads bank data with (D-060).
//!
//! This is the only crate that talks to the two C libraries, and it holds no
//! logic beyond loading them, checking their versions and copying their
//! answers out safely. Everything else, from the bank abstraction to
//! turning a transaction into a statement, lives in `kassensturz-bank`
//! (D-060), the only crate that uses this one (`docs/ARCHITECTURE.md`,
//! dependency rules).
//!
//! # How the libraries are reached
//!
//! The declarations in [`gwenhywfar`] and [`aqbanking`] are generated once
//! with bindgen in its dynamic-loading mode and committed
//! (`bindgen/generate.sh`), so a normal build needs neither bindgen nor the
//! C headers. At runtime [`Libraries::load`] opens `libgwenhywfar.so.79` and
//! `libaqbanking.so.44` by these ABI names with `libloading`, so KassenSturz
//! starts and works without them (D-054), and checks the versions they
//! report before anything else is called: Gwenhywfar 5.14.1 and AqBanking
//! 6.9.2 or newer under the same ABI name pass; anything older counts as
//! "not installed" (D-138, threat T-17). With `--dynamic-link-require-all`
//! one missing function also makes the whole library count as not installed,
//! so a different build can never be half-used.
//!
//! The order of calls a user of this crate follows: `GWEN_Init`, then
//! `AB_Banking_new` with KassenSturz's own data folder (D-139) and
//! `AB_Banking_Init`; at the end `AB_Banking_Fini`, `AB_Banking_free`,
//! `GWEN_Fini`. A [`Libraries`] value must outlive every pointer obtained
//! through it: dropping it unloads the libraries. [`session::Session`]
//! does all of this behind a safe API (M4.5): it is the only code that
//! calls the declarations, reads every answer into plain Rust values
//! under the rules of threat T-09 (null checks, bounded copies, one rule
//! for text that isn't UTF-8), and routes Gwenhywfar's callbacks (PIN,
//! password status, log, certificate) to a handler the app supplies.
//! `kassensturz-bank` builds the bank backend on it without unsafe code.
//!
//! # What the declarations leave out, on purpose
//!
//! - Every floating-point amount function of `AB_VALUE`
//!   (`AB_Value_GetValueAsDouble`, `AB_Value_SetValueFromDouble`,
//!   `AB_Value_fromDouble`, `AB_Value_toDbFloat`) and the lossy `long`
//!   accessors `AB_Value_Num` and `AB_Value_Denom`: amounts are read only as
//!   the exact fraction's text, `AB_Value_GetNumDenomString` (D-141, T-10).
//! - Every command that moves money (D-053, T-14): the transfer, debit note,
//!   standing order, dated transfer and phone top-up variants of
//!   `AB_TRANSACTION_COMMAND` and the setters only they need. What remains
//!   are the reading commands: balances, transactions, standing orders and
//!   dated transfers as they stand, electronic statements, depots.
//! - Gwenhywfar's password store and password database
//!   (`GWEN_Gui_SetPasswdStore`, `GWEN_Gui_SetPasswordDb`): a PIN is
//!   answered by KassenSturz's own callbacks and never saved by Gwenhywfar
//!   (D-140, T-01).
//! - AqBanking's file importers and exporters, securities, documents and
//!   transaction limits: KassenSturz has its own importers and reads only.
//!
//! # Setting up a bank login
//!
//! AqBanking 6.9.2 exports none of its backend-support API (`AB_Provider_*`,
//! `AB_User_*`, `AB_Account_*` are hidden symbols in `libaqbanking.so.44`), so
//! a login can only be set up through the public entry points that are
//! declared here: `AB_Banking_ProviderControl`, which runs the FinTS
//! backend's commands (`adduser`, `getsysid`, `getaccounts`, …, the same
//! ones `aqhbci-tool4` offers) with an argument list, for KassenSturz's own
//! IBAN-first setup flow, and `AB_Banking_GetNewUserDialog` plus
//! `AB_Banking_CreateSetupDialog` for Gwenhywfar's dialogs as the fallback
//! (Q-035, D-144).
//!
//! # Gwenhywfar's dialogs
//!
//! Bank setup, PIN and TAN dialogs come from Gwenhywfar's Qt 6 library,
//! which is C++ and so can't be declared here. KassenSturz's own small C++
//! library `kassensturz-gwengui` (in `gwengui/`, built by CMake when
//! `gwengui-qt6` is installed) wraps it behind the C functions declared in
//! [`gwengui`], and is loaded at runtime the same way (Q-035 option A,
//! D-140).
//!
//! Secrets never appear here: the crate has no test data, and the tests load
//! the real libraries only to read their version numbers (D-055).

pub mod aqbanking;
pub mod gwengui;
pub mod gwenhywfar;
mod load;
pub mod session;
#[cfg(feature = "stub")]
pub mod stub;
mod text;

pub use load::{AQBANKING_LIBRARY, GWENHYWFAR_LIBRARY, Libraries, Library, LoadError, Version};
pub use text::MAX_TEXT_BYTES;

/// Loads the installed libraries only to read their versions (D-054,
/// D-138), then unloads them again. This is how the app learns whether
/// bank connections are possible without opening a session.
pub fn probe() -> Result<(Version, Version), LoadError> {
    // SAFETY: loading runs the libraries' initialisers; nothing is called
    // but the version functions, and no pointer outlives the value.
    let libraries = unsafe { Libraries::load() }?;
    Ok((
        libraries.gwenhywfar_version(),
        libraries.aqbanking_version(),
    ))
}

/// [`probe`] for libraries at the given paths, e.g. a stub library in a
/// test.
pub fn probe_from(
    gwenhywfar: &std::path::Path,
    aqbanking: &std::path::Path,
) -> Result<(Version, Version), LoadError> {
    // SAFETY: as in `probe`.
    let libraries = unsafe { Libraries::load_from(gwenhywfar, aqbanking) }?;
    Ok((
        libraries.gwenhywfar_version(),
        libraries.aqbanking_version(),
    ))
}
