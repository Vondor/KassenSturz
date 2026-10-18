// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! IDs. Everything in a book is identified by a UUID, version 7, which begins
//! with a timestamp (D-067).

/// Defines a newtype ID around a UUID, so IDs of different things can't be
/// mixed up.
macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(uuid::Uuid);

        impl $name {
            /// Creates a new, unique ID: a UUIDv7, which begins with the
            /// current time.
            pub fn generate() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            /// Wraps the 16 bytes of an ID, e.g. one read back from a book.
            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(uuid::Uuid::from_bytes(bytes))
            }

            /// The ID's 16 bytes, as stored in a book.
            pub const fn to_bytes(self) -> [u8; 16] {
                self.0.into_bytes()
            }
        }

        impl core::fmt::Display for $name {
            /// The usual hyphenated form, e.g. for log messages.
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                self.0.hyphenated().fmt(f)
            }
        }
    };
}

pub(crate) use define_id;
