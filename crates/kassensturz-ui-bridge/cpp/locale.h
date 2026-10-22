// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Parts of the user's regional format, read from Qt's locale data. The Rust
// side formats every amount itself from the exact value (D-065), so only
// integers and text cross this boundary, never floating point (AGENTS.md
// §3.1 rule 1).

#pragma once

#include <QtCore/QString>

#include <cstdint>

namespace kassensturz::locale
{
// The separator between the whole and the fractional part, e.g. "," in German.
QString decimalPoint();

// The locale's digit zero: "0" in most locales, "٠" in Arabic.
QString zeroDigit();

// The ISO 4217 code of the locale's own currency, e.g. "EUR". Empty if the
// locale has none.
QString currencyIsoCode();

// The symbol of the locale's own currency, e.g. "€".
QString currencySymbol();

// `value` with the locale's digit grouping and digits, e.g. "1.234.567" in
// German.
QString groupDigits(std::uint64_t value);

// What separates groups of digits, e.g. "." in German.
QString groupSeparator();

// The text before the digits of an amount written with `symbol`, e.g. "-" for
// a negative amount in German, or "-€" in American English.
QString currencyPrefix(bool negative, const QString &symbol);

// The text after the digits of an amount written with `symbol`, e.g. " €" in
// German.
QString currencySuffix(bool negative, const QString &symbol);

// A date in the locale's short format, always with a four-digit year.
QString shortDate(std::int32_t year, std::int32_t month, std::int32_t day);

// A date typed in the locale's short format, with a four- or two-digit year,
// as ISO 8601 ("2026-10-05"). Empty if the text isn't such a date.
QString readShortDate(const QString &text);

// The month's name with its year in the locale's language, e.g.
// "October 2026" or "Oktober 2026".
QString monthAndYear(std::int32_t year, std::int32_t month);

// Today's date on this computer, as ISO 8601.
QString today();

// The local date and time `seconds` after 1970-01-01 00:00 UTC, in the
// locale's short format.
QString dateTime(std::int64_t seconds);

// Makes the locale `name` (e.g. "de_DE") the one used from now on.
void setDefaultLocale(const QString &name);
}
