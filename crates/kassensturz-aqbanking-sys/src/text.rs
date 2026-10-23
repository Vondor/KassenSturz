// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Copying text out of the C libraries (threat T-09).
//!
//! Every string AqBanking hands over is read here and nowhere else, with
//! one rule for each thing that can go wrong:
//!
//! - a null pointer is "no text" (`None`), never a crash;
//! - the text is copied at once, at most [`MAX_TEXT_BYTES`] bytes of it,
//!   so a missing terminator can't run past the end and an absurdly long
//!   value can't fill the memory; what is longer is cut, never refused,
//!   since a long purpose line mustn't stop an import;
//! - bytes that aren't valid UTF-8 are replaced by U+FFFD, the replacement
//!   character, so the text stays readable and nothing is guessed about its
//!   encoding (AqBanking converts the bank's text to UTF-8; what still
//!   isn't is kept as data, never refused).

use std::os::raw::c_char;

/// The most bytes copied from one C string. The longest FinTS texts are
/// purpose lines of a few hundred characters; the bank's message texts
/// are longer but never this long.
pub const MAX_TEXT_BYTES: usize = 4096;

/// Copies the C string `pointer` points to, following the rules above.
///
/// # Safety
///
/// `pointer` is null or points to readable memory that is terminated by a
/// zero byte within [`MAX_TEXT_BYTES`] bytes, or is readable for at least
/// that many bytes.
pub(crate) unsafe fn copy_text(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let mut bytes = Vec::new();
    for offset in 0..MAX_TEXT_BYTES {
        // SAFETY: the caller promises the memory is readable up to the
        // terminator or for MAX_TEXT_BYTES bytes; `offset` stays below that.
        let byte = unsafe { pointer.add(offset).read() };
        if byte == 0 {
            break;
        }
        bytes.push(byte.cast_unsigned());
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Copies the text from a buffer the library filled, up to the first zero
/// byte or `len` bytes.
pub(crate) fn text_from_buffer(buffer: &[u8]) -> String {
    let end = buffer
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn a_null_pointer_is_no_text() {
        // SAFETY: null is allowed.
        assert_eq!(unsafe { copy_text(std::ptr::null()) }, None);
    }

    #[test]
    fn text_is_copied_whole() {
        let text = CString::new("Miete Oktober").expect("no zero byte");
        // SAFETY: a terminated C string.
        assert_eq!(
            unsafe { copy_text(text.as_ptr()) }.as_deref(),
            Some("Miete Oktober")
        );
    }

    #[test]
    fn bytes_that_are_not_utf8_become_the_replacement_character() {
        // "Caf" followed by a lone Latin-1 "é" byte, as a bank in the wrong
        // charset would send it.
        let bytes = b"Caf\xe9 Berlin".to_vec();
        let text = CString::new(bytes).expect("no zero byte");
        // SAFETY: a terminated C string.
        assert_eq!(
            unsafe { copy_text(text.as_ptr()) }.as_deref(),
            Some("Caf\u{FFFD} Berlin")
        );
    }

    #[test]
    fn text_longer_than_the_limit_is_cut_not_refused() {
        let long = "x".repeat(MAX_TEXT_BYTES + 100);
        let text = CString::new(long).expect("no zero byte");
        // SAFETY: a terminated C string, longer than the limit.
        let copied = unsafe { copy_text(text.as_ptr()) }.expect("text");
        assert_eq!(copied.len(), MAX_TEXT_BYTES);
    }

    #[test]
    fn a_buffer_is_read_up_to_its_terminator() {
        assert_eq!(text_from_buffer(b"12345/100\0garbage"), "12345/100");
        assert_eq!(text_from_buffer(b"no terminator"), "no terminator");
    }
}
