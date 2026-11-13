// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The shape of a FinTS product registration number, in a file of its own
// because two places need the same rule: the crate, for a number typed
// while the app runs (`RegistrationKey`, D-260), and `build.rs`, for a
// number built into an official release (D-745). `build.rs` takes this
// file in with `include!`, so it holds nothing but these two items.

/// How many characters a FinTS product registration number has.
pub const REGISTRATION_KEY_LENGTH: usize = 25;

/// Whether `key` is exactly [`REGISTRATION_KEY_LENGTH`] ASCII letters and
/// digits. Nothing else is let through, so FinTS's own separators (`'`,
/// `+`, `:`, `?`) can't get into a message to the bank.
#[must_use]
pub fn is_registration_shape(key: &str) -> bool {
    key.len() == REGISTRATION_KEY_LENGTH && key.bytes().all(|b| b.is_ascii_alphanumeric())
}
