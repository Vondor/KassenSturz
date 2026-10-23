// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only bank connections for KassenSturz (D-012, D-053, D-060).
//!
//! This is the safe layer every other crate uses. It holds:
//!
//! - the bank abstraction, [`Bank`]: logins, accounts, balance and booked
//!   transactions, credit cards included, and nothing that moves money
//!   (D-053, threat T-14);
//! - what the bank delivers, [`Fetched`], and its conversion into the
//!   importer's [`kassensturz_import::Statement`] with exact amounts
//!   (D-141), so a bank connection feeds the same import pipeline as a file
//!   (D-030, threat T-11);
//! - the way from an IBAN to the bank and to what its login needs
//!   ([`Iban`], [`BankDirectory`], [`setup_for`]), with every gap in the
//!   sources as an explicit state ([`Setup`], D-144) and the hand-checked
//!   bank list for corrections ([`banks`], D-145, `docs/BANK_LIST.md`);
//! - the PIN and the user's choice how long to keep it ([`Pin`],
//!   [`PinMode`], [`PinSession`], D-057), with a wrong PIN forgotten at once
//!   (D-140, threat T-05);
//! - whether AqBanking is there and new enough ([`Availability`], D-054,
//!   D-138);
//! - the fake bank for tests, [`FakeBank`] (D-055).
//!
//! The real backend over AqBanking comes in M4.5, on top of the raw
//! declarations in `kassensturz-aqbanking-sys` (M4.2); this crate has no
//! unsafe code and opens no network connection itself.

#![forbid(unsafe_code)]

mod availability;
mod backend;
pub mod banks;
mod directory;
mod error;
mod fake;
mod fetched;
mod iban;
mod pin;
mod statement;

pub use availability::{
    AQBANKING_LIBRARY, AQBANKING_MINIMUM, Availability, FINTS_REGISTRATION_KEY,
    GWENGUI_QT6_LIBRARY, GWENHYWFAR_LIBRARY, GWENHYWFAR_MINIMUM, Library, Version,
};
pub use backend::{
    Bank, BankAccount, BankAccountId, BankAccountKind, Login, LoginId, LoginSetup, remove_login,
};
pub use directory::{
    BankDirectory, BankInfo, Corrected, CustomerId, FintsServer, HandCheckedList, LoginKind,
    Question, Setup, setup_for,
};
pub use error::BankError;
pub use fake::{Failure, FakeBank, FakeDirectory};
pub use fetched::{Fetched, FetchedAmount, FetchedBalance, FetchedTransaction};
pub use iban::{BankCode, Iban};
pub use pin::{MemoryPinStore, Pin, PinAnswer, PinMode, PinSession, PinStore};
pub use statement::statement;
