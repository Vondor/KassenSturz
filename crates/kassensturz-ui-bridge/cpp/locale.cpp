// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "kassensturz-ui-bridge/cpp/locale.h"

#include <QtCore/QDate>
#include <QtCore/QLatin1StringView>
#include <QtCore/QLocale>

namespace kassensturz::locale
{
namespace
{
struct Affixes {
    QString prefix;
    QString suffix;
};

// Splits the locale's currency format into the text before and after the
// digits, by formatting a sample number with Qt and finding its digits in
// the result. The sample is an integer, so none of QLocale's floating-point
// functions are involved.
// The locale's short date format, always with a four-digit year: many short
// formats use two-digit years ("05.10.26"), which are easy to misread in a
// household book.
QString shortDateFormat(const QLocale &locale)
{
    QString format = locale.dateFormat(QLocale::ShortFormat);
    if (!format.contains(QLatin1StringView("yyyy"))) {
        format.replace(QLatin1StringView("yy"), QLatin1StringView("yyyy"));
    }
    return format;
}

// `format` without leading zeros on day and month, which Qt then reads with
// or without them: "5.10.2026" is as clear as "05.10.2026".
QString withoutLeadingZeros(QString format)
{
    if (!format.contains(QLatin1StringView("MMM"))) {
        format.replace(QLatin1StringView("MM"), QLatin1StringView("M"));
    }
    if (!format.contains(QLatin1StringView("ddd"))) {
        format.replace(QLatin1StringView("dd"), QLatin1StringView("d"));
    }
    return format;
}

Affixes currencyAffixes(bool negative, const QString &symbol)
{
    const QLocale locale;
    constexpr qlonglong sample = 1234567;
    const QString digits = locale.toString(sample);
    const QString formatted = locale.toCurrencyString(negative ? -sample : sample, symbol);
    const qsizetype at = formatted.indexOf(digits);
    if (at < 0) {
        // Not expected for any locale Qt knows; the symbol after the number
        // is a readable fallback.
        return {negative ? locale.negativeSign() : QString(), QStringLiteral(" ") + symbol};
    }
    return {formatted.left(at), formatted.mid(at + digits.size())};
}
}

QString decimalPoint()
{
    return QLocale().decimalPoint();
}

QString zeroDigit()
{
    return QLocale().zeroDigit();
}

QString currencyIsoCode()
{
    return QLocale().currencySymbol(QLocale::CurrencyIsoCode);
}

QString currencySymbol()
{
    return QLocale().currencySymbol(QLocale::CurrencySymbol);
}

QString groupDigits(std::uint64_t value)
{
    return QLocale().toString(static_cast<qulonglong>(value));
}

QString groupSeparator()
{
    return QLocale().groupSeparator();
}

QString currencyPrefix(bool negative, const QString &symbol)
{
    return currencyAffixes(negative, symbol).prefix;
}

QString currencySuffix(bool negative, const QString &symbol)
{
    return currencyAffixes(negative, symbol).suffix;
}

QString shortDate(std::int32_t year, std::int32_t month, std::int32_t day)
{
    const QLocale locale;
    return locale.toString(QDate(year, month, day), shortDateFormat(locale));
}

QString readShortDate(const QString &text)
{
    const QLocale locale;
    const QString trimmed = text.trimmed();
    QDate date = locale.toDate(trimmed, withoutLeadingZeros(shortDateFormat(locale)));
    if (!date.isValid()) {
        // The locale's own short format, which may have a two-digit year:
        // "05.10.26" is read as 2026, not 1926.
        constexpr int baseYear = 2000;
        date = locale.toDate(trimmed, withoutLeadingZeros(locale.dateFormat(QLocale::ShortFormat)), baseYear);
    }
    return date.isValid() ? date.toString(Qt::ISODate) : QString();
}

QString today()
{
    return QDate::currentDate().toString(Qt::ISODate);
}

void setDefaultLocale(const QString &name)
{
    QLocale::setDefault(QLocale(name));
}
}
