// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Amounts as text, exactly as stored.

/// The minor units `units` of a currency with `digits` decimal places as
/// text, with `mark` as the decimal mark: 1234 and 2 give `12.34`, -5 and 2
/// give `-0.05`. Exact integer arithmetic (AGENTS.md §3.1 rule 1). `None` if
/// `digits` is more than a 64-bit amount can have.
pub(crate) fn decimal_text(units: i64, digits: u8, mark: char) -> Option<String> {
    let magnitude = u128::from(units.unsigned_abs());
    let sign = if units < 0 { "-" } else { "" };
    if digits == 0 {
        return Some(format!("{sign}{magnitude}"));
    }
    let scale = 10_u128.checked_pow(u32::from(digits))?;
    Some(format!(
        "{sign}{}{mark}{:0width$}",
        magnitude / scale,
        magnitude % scale,
        width = usize::from(digits)
    ))
}
