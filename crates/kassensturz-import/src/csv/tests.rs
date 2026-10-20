// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The CSV importer's parts. Whole files are in the golden files
//! (`tests/golden/csv/`).

use kassensturz_core::{CurrencyCode, Date};
use proptest::prelude::*;

use super::text::{Rows, decode, without_first_lines, without_last_lines};
use super::values::{parse_csv_amount, parse_date};
use super::*;
use crate::amount::DecimalMark;
use crate::importer::import;

fn eur() -> Currency {
    Currency::new(CurrencyCode::new("EUR").expect("a valid code"), 2).expect("valid places")
}

fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("a valid date")
}

fn cents(text: &str, mark: DecimalMark) -> Result<i64, ImportError> {
    parse_csv_amount(text, mark, eur()).map(Money::minor_units)
}

/// Simple settings: `;`, decimal comma, day first, columns 1 and 2.
fn simple() -> CsvSettings {
    let mut settings = CsvSettings::new(
        ';',
        DecimalMark::Comma,
        DateOrder::DayMonthYear,
        Column::Number(1),
        AmountColumns::Signed(Column::Number(2)),
    );
    settings.header = false;
    settings
}

fn read(settings: CsvSettings, input: &str) -> Result<Vec<Statement>, ImportError> {
    let importer = Csv::new(settings, eur())?;
    match import(&importer, input.as_bytes(), &Limits::default())? {
        Imported::Statements(statements) => Ok(statements),
        other => panic!("a statement importer read {other:?}"),
    }
}

#[test]
fn dates_are_read_in_the_chosen_order() {
    let october = date(2026, 10, 5);
    for (text, order) in [
        ("05.10.2026", DateOrder::DayMonthYear),
        ("5.10.26", DateOrder::DayMonthYear),
        (" 05102026 ", DateOrder::DayMonthYear),
        ("10/05/2026", DateOrder::MonthDayYear),
        ("10/5/26", DateOrder::MonthDayYear),
        ("2026-10-05", DateOrder::YearMonthDay),
        ("20261005", DateOrder::YearMonthDay),
        ("26-10-05", DateOrder::YearMonthDay),
    ] {
        assert_eq!(parse_date(text, order), Ok(october), "{text}");
    }
    // Two-digit years as POSIX reads them.
    assert_eq!(
        parse_date("31.12.69", DateOrder::DayMonthYear),
        Ok(date(1969, 12, 31))
    );
    assert_eq!(
        parse_date("01.01.68", DateOrder::DayMonthYear),
        Ok(date(2068, 1, 1))
    );
}

#[test]
fn a_date_that_does_not_fit_the_order_is_refused() {
    for (text, order) in [
        ("10/25/2026", DateOrder::DayMonthYear),
        ("25.10.2026", DateOrder::MonthDayYear),
        ("2026-02-30", DateOrder::YearMonthDay),
        ("05.10-2026", DateOrder::DayMonthYear),
        ("05.10.202", DateOrder::DayMonthYear),
        ("005.10.2026", DateOrder::DayMonthYear),
        ("5.10.2026 14:02", DateOrder::DayMonthYear),
        ("05 10 2026", DateOrder::DayMonthYear),
        ("", DateOrder::DayMonthYear),
        ("+5.10.2026", DateOrder::DayMonthYear),
        ("٥.١٠.٢٠٢٦", DateOrder::DayMonthYear),
    ] {
        assert_eq!(
            parse_date(text, order),
            Err(ImportError::BadDate {
                found: text.to_owned(),
                line: None
            }),
            "{text}"
        );
    }
}

#[test]
fn amounts_are_read_with_their_grouping() {
    use DecimalMark::{Comma, Point};
    for (text, mark, expected) in [
        ("-1.234,56", Comma, -123_456),
        ("1.234.567,8", Comma, 123_456_780),
        ("1 234,50", Comma, 123_450),
        ("1\u{a0}234,50", Comma, 123_450),
        ("1\u{202f}234", Comma, 123_400),
        ("1.234", Comma, 123_400),
        (" +12,5 ", Comma, 1_250),
        ("0,01", Comma, 1),
        ("1,234.56", Point, 123_456),
        ("1'234.56", Point, 123_456),
        ("-0.99", Point, -99),
        ("7", Point, 700),
    ] {
        assert_eq!(cents(text, mark), Ok(expected), "{text}");
    }
}

#[test]
fn an_amount_with_the_other_decimal_mark_is_refused_not_misread() {
    use DecimalMark::{Comma, Point};
    for (text, mark) in [
        ("12.50", Comma),
        ("1234.56", Comma),
        ("1,234.56", Comma),
        ("12,50", Point),
        ("1.234,56", Point),
        ("1.23,00", Comma),
        ("1.2345,00", Comma),
        ("1. 234,00", Comma),
        (".234,00", Comma),
        ("1.234 567,00", Comma),
        ("- 12,50", Comma),
        ("12,50-", Comma),
        ("12,50 €", Comma),
        ("(12.50)", Point),
        ("", Comma),
    ] {
        assert_eq!(
            cents(text, mark),
            Err(ImportError::BadAmount {
                found: text.to_owned(),
                line: None
            }),
            "{text}"
        );
    }
    assert_eq!(
        cents("1,005", Comma),
        Err(ImportError::TooManyDecimalPlaces {
            found: "1,005".to_owned(),
            currency: eur().code(),
            line: None
        })
    );
}

#[test]
fn windows_1252_and_iso_8859_15_are_decoded_by_their_tables() {
    let windows = decode(
        b"\x80 \x84\x93\x94 \xe4\xdf \xa4",
        Some(TextEncoding::Windows1252),
    );
    assert_eq!(windows.as_deref(), Ok("€ „“” äß ¤"));
    let latin9 = decode(
        b"\xa4 \xa6\xa8\xb4\xb8\xbc\xbd\xbe \xe4",
        Some(TextEncoding::Iso8859_15),
    );
    assert_eq!(latin9.as_deref(), Ok("€ ŠšŽžŒœŸ ä"));
    // Every byte Windows-1252 has a character for, and only those.
    for byte in 0x80_u8..=0xFF {
        let input = [byte];
        let undefined = matches!(byte, 0x81 | 0x8D | 0x8F | 0x90 | 0x9D);
        assert_eq!(
            decode(&input, Some(TextEncoding::Windows1252)).is_err(),
            undefined,
            "{byte:#x}"
        );
    }
}

#[test]
fn a_byte_without_a_character_names_the_encoding_and_line() {
    assert_eq!(
        decode(b"a\nb\n\x9d", Some(TextEncoding::Windows1252)),
        Err(ImportError::WrongEncoding {
            encoding: TextEncoding::Windows1252,
            line: Some(3)
        })
    );
    // ISO-8859-15 has no characters at 0x80 to 0x9F: a euro sign there is
    // Windows-1252's.
    assert_eq!(
        decode(b"\x80", Some(TextEncoding::Iso8859_15)),
        Err(ImportError::WrongEncoding {
            encoding: TextEncoding::Iso8859_15,
            line: Some(1)
        })
    );
    assert_eq!(
        decode(b"x\n\xe4", Some(TextEncoding::Utf8)),
        Err(ImportError::WrongEncoding {
            encoding: TextEncoding::Utf8,
            line: Some(2)
        })
    );
}

#[test]
fn without_an_encoding_utf_8_comes_first_and_a_byte_order_mark_decides() {
    assert_eq!(decode("ä".as_bytes(), None).as_deref(), Ok("ä"));
    assert_eq!(decode(b"\xe4", None).as_deref(), Ok("ä"));
    assert_eq!(
        decode(b"\xef\xbb\xbf\xc3\xa4", Some(TextEncoding::Windows1252)).as_deref(),
        Ok("ä")
    );
    assert_eq!(
        decode(b"a\n\0", None),
        Err(ImportError::Malformed { line: Some(2) })
    );
}

fn rows(text: &str, delimiter: char) -> Vec<(u64, Vec<String>)> {
    let mut rows = Rows::new(text, delimiter, 1);
    let mut all = Vec::new();
    while let Some(row) = rows.next_row() {
        let row = row.expect("a valid row");
        all.push((
            row.line,
            row.fields.iter().map(|field| field.to_string()).collect(),
        ));
    }
    all
}

fn fields(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn rows_are_split_as_rfc_4180_says() {
    assert_eq!(
        rows("a;\"b;c\";\"d\"\"e\"\r\n\"multi\nline\";x\n;\nlast", ';'),
        vec![
            (1, fields(&["a", "b;c", "d\"e"])),
            (2, fields(&["multi\nline", "x"])),
            (4, fields(&["", ""])),
            (5, fields(&["last"])),
        ]
    );
    // A quote inside an unquoted field is text; a lone `\r` too.
    assert_eq!(
        rows("5\" screen\tx\ry\n", '\t'),
        vec![(1, fields(&["5\" screen", "x\ry"]))]
    );
    // An empty quoted field at the end.
    assert_eq!(rows("a,\"\"\r\n", ','), vec![(1, fields(&["a", ""]))]);
}

#[test]
fn a_broken_quote_fails_with_its_line() {
    for text in ["a\n\"b;c\nd\n", "a\n\"b\"c;d\n"] {
        let mut rows = Rows::new(text, ';', 1);
        assert!(rows.next_row().is_some_and(|row| row.is_ok()));
        assert_eq!(
            rows.next_row().map(|row| row.err()),
            Some(Some(ImportError::Malformed { line: Some(2) })),
            "{text:?}"
        );
    }
}

#[test]
fn a_row_with_too_many_fields_is_refused() {
    let text = ";".repeat(MAX_COLUMNS);
    let mut rows = Rows::new(&text, ';', 1);
    assert_eq!(
        rows.next_row().map(|row| row.err()),
        Some(Some(ImportError::Malformed { line: Some(1) }))
    );
    let text = ";".repeat(MAX_COLUMNS - 1);
    assert!(
        Rows::new(&text, ';', 1)
            .next_row()
            .is_some_and(|row| row.is_ok())
    );
}

#[test]
fn lines_are_skipped_at_the_start_and_the_end() {
    let text = "one\ntwo\nthree\n\nfour\n\n";
    assert_eq!(without_first_lines(text, 2), "three\n\nfour\n\n");
    assert_eq!(without_first_lines(text, 9), "");
    assert_eq!(without_last_lines(text, 1), "one\ntwo\nthree\n\n");
    assert_eq!(without_last_lines(text, 2), "one\ntwo\n");
    assert_eq!(without_last_lines(text, 9), "");
    assert_eq!(without_last_lines("one\ntwo", 1), "one\n");
}

#[test]
fn skipped_lines_keep_the_line_numbers_of_the_file() {
    let mut settings = simple();
    settings.skip_lines = 2;
    let error = read(settings, "Account\nPeriod\n01.10.2026;1,00\n02.10.2026;x\n");
    assert_eq!(
        error,
        Err(ImportError::BadAmount {
            found: "x".to_owned(),
            line: Some(4)
        })
    );
}

#[test]
fn rows_without_a_column_leave_it_empty() {
    let mut settings = simple();
    settings.other_party_name = Some(Column::Number(5));
    settings.remittance_information = vec![Column::Number(3), Column::Number(4)];
    let statements = read(settings, "01.10.2026;-1,00;;  \n").expect("read");
    let entry = &statements[0].entries[0];
    assert_eq!(entry.other_party_name, None);
    assert_eq!(entry.remittance_information, None);
    assert_eq!(entry.amount.minor_units(), -100);
}

#[test]
fn every_row_counts_against_the_entry_limit() {
    let importer = Csv::new(simple(), eur()).expect("valid settings");
    let limits = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    let input = "01.10.2026;1,00\n02.10.2026;1,00\n03.10.2026;1,00\n";
    assert_eq!(
        importer.read(input.as_bytes(), &limits),
        Err(ImportError::TooManyEntries { max: 2 })
    );
}

#[test]
fn a_long_text_is_refused_with_its_line() {
    let mut settings = simple();
    settings.other_party_name = Some(Column::Number(3));
    let importer = Csv::new(settings, eur()).expect("valid settings");
    let limits = Limits {
        max_text_bytes: 4,
        ..Limits::default()
    };
    let input = "01.10.2026;1,00;Kiosk\n";
    assert_eq!(
        importer.read(input.as_bytes(), &limits),
        Err(ImportError::TextTooLong {
            max_bytes: 4,
            line: Some(1)
        })
    );
}

#[test]
fn settings_that_cannot_work_are_refused() {
    let mut cases = Vec::new();
    let mut quote = simple();
    quote.delimiter = '"';
    cases.push(quote);
    let mut zero = simple();
    zero.booking_date = Column::Number(0);
    cases.push(zero);
    let mut too_far = simple();
    too_far.value_date = Some(Column::Number(MAX_COLUMNS + 1));
    cases.push(too_far);
    let mut name_without_header = simple();
    name_without_header.other_party_name = Some(Column::Name("Name".to_owned()));
    cases.push(name_without_header);
    let mut nothing_booked = simple();
    nothing_booked.status = Some(StatusColumn {
        column: Column::Number(3),
        booked: Vec::new(),
    });
    cases.push(nothing_booked);
    let mut same_direction = simple();
    same_direction.amount = AmountColumns::WithDirection {
        amount: Column::Number(2),
        direction: Column::Number(3),
        money_out: "S".to_owned(),
        money_in: " S ".to_owned(),
    };
    cases.push(same_direction);
    for settings in cases {
        assert_eq!(
            Csv::new(settings.clone(), eur()),
            Err(ImportError::BadSettings { line: None }),
            "{settings:?}"
        );
    }
}

#[test]
fn settings_survive_their_text_form() {
    let mut settings = CsvSettings::new(
        '\t',
        DecimalMark::Point,
        DateOrder::MonthDayYear,
        Column::Name("Date \"booked\"\\".to_owned()),
        AmountColumns::WithDirection {
            amount: Column::Number(4),
            direction: Column::Name("Dr/Cr".to_owned()),
            money_out: "Dr".to_owned(),
            money_in: "Cr".to_owned(),
        },
    );
    settings.encoding = Some(TextEncoding::Iso8859_15);
    settings.skip_lines = 3;
    settings.skip_last_lines = 1;
    settings.value_date = Some(Column::Number(2));
    settings.other_party_name = Some(Column::Name("Payee".to_owned()));
    settings.other_party_iban = Some(Column::Number(7));
    settings.remittance_information =
        vec![Column::Name("Memo\nline".to_owned()), Column::Number(9)];
    settings.status = Some(StatusColumn {
        column: Column::Number(10),
        booked: vec!["Posted".to_owned(), "Cleared".to_owned()],
    });
    settings.currency = Some(Column::Name("Currency".to_owned()));
    settings.account = Some(Column::Number(1));
    let text = settings.to_text();
    assert_eq!(CsvSettings::from_text(&text), Ok(settings));
    for settings in [
        simple(),
        CsvSettings::new(
            ',',
            DecimalMark::Comma,
            DateOrder::YearMonthDay,
            Column::Number(1),
            AmountColumns::DebitCredit {
                debit: Column::Number(2),
                credit: Column::Number(3),
            },
        ),
    ] {
        assert_eq!(CsvSettings::from_text(&settings.to_text()), Ok(settings));
    }
}

#[test]
fn a_settings_text_says_which_line_is_wrong() {
    let base = "delimiter = \";\"\ndecimal-mark = comma\ndate-order = day-month-year\n\
                booking-date = 1\namount = 2\n";
    assert!(CsvSettings::from_text(base).is_ok());
    for (extra, line) in [
        ("amount = 3", 6),
        ("colour = blue", 6),
        ("header = maybe", 6),
        ("encoding = latin-1", 6),
        ("skip-lines = -1", 6),
        ("value-date = Valuta", 6),
        ("other-party-name = \"open", 6),
        ("other-party-name = \"a\"b\"", 6),
        ("other-party-name = \"\\x\"", 6),
        ("delimiter-two = \";;\"", 6),
        ("no equals sign", 6),
    ] {
        assert_eq!(
            CsvSettings::from_text(&format!("{base}{extra}\n")),
            Err(ImportError::BadSettings { line: Some(line) }),
            "{extra}"
        );
    }
    // Comments and empty lines are left out, but count as lines.
    assert_eq!(
        CsvSettings::from_text(&format!("# Bank\n\n{base}delimiter = \",\"\n")),
        Err(ImportError::BadSettings { line: Some(8) })
    );
    // Missing or contradicting keys have no line.
    for text in [
        "delimiter = \";\"\n",
        &format!("{base}debit = 3\n"),
        &format!("{base}booked = \"Gebucht\"\n"),
        &format!("{base}money-out = \"S\"\n"),
        &base.replace("delimiter = \";\"", "delimiter = \"\\\"\""),
    ] {
        assert_eq!(
            CsvSettings::from_text(text),
            Err(ImportError::BadSettings { line: None }),
            "{text}"
        );
    }
}

#[test]
fn recognition_needs_the_named_columns() {
    let mut settings = CsvSettings::new(
        ';',
        DecimalMark::Comma,
        DateOrder::DayMonthYear,
        Column::Name("Datum".to_owned()),
        AmountColumns::Signed(Column::Name("Betrag".to_owned())),
    );
    settings.skip_lines = 1;
    let importer = Csv::new(settings, eur()).expect("valid settings");
    assert_eq!(
        importer.recognize(b"Konto: 1\nDatum;Name;Betrag\n"),
        Recognition::Yes
    );
    assert_eq!(
        importer.recognize(b"Konto: 1\nDatum;Name\n"),
        Recognition::No
    );
    assert_eq!(importer.recognize(b"Konto: 1\n"), Recognition::Maybe);
    assert_eq!(importer.recognize(b"\0\0"), Recognition::No);
    // A start cut off in the middle of a character still counts.
    let mut start = b"Konto: 1\nDatum;Name;Betrag\n".to_vec();
    start.resize(RECOGNITION_BYTES - 1, b'x');
    start.extend_from_slice("ä".as_bytes());
    assert_eq!(
        importer.recognize(&start[..RECOGNITION_BYTES]),
        Recognition::Yes
    );
    let numbered = Csv::new(simple(), eur()).expect("valid settings");
    assert_eq!(numbered.recognize(b"01.10.2026;1,00\n"), Recognition::Maybe);
    assert_eq!(numbered.recognize(b"<?xml?>\n"), Recognition::No);
}

/// An amount in cents, written as a bank might: with or without grouping,
/// with either decimal mark.
fn written(cents: i64, mark: DecimalMark, grouping: Option<char>) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let magnitude = cents.unsigned_abs();
    let whole = (magnitude / 100).to_string();
    let whole = match grouping {
        Some(separator) => {
            let digits: Vec<char> = whole.chars().collect();
            let mut out = String::new();
            for (index, digit) in digits.iter().enumerate() {
                if index > 0 && (digits.len() - index).is_multiple_of(3) {
                    out.push(separator);
                }
                out.push(*digit);
            }
            out
        }
        None => whole,
    };
    let mark = match mark {
        DecimalMark::Point => '.',
        DecimalMark::Comma => ',',
    };
    format!("{sign}{whole}{mark}{:02}", magnitude % 100)
}

proptest! {
    #[test]
    fn every_amount_is_read_exactly(
        cents in -1_000_000_000_000_i64..1_000_000_000_000,
        comma in any::<bool>(),
        grouping in prop::sample::select(vec![None, Some(' '), Some('\''), Some('\u{a0}'), Some('.')]),
    ) {
        let mark = if comma { DecimalMark::Comma } else { DecimalMark::Point };
        // The other mark groups digits; the same one can't.
        let grouping = grouping.map(|separator| match (separator, mark) {
            ('.', DecimalMark::Point) => ',',
            (separator, _) => separator,
        });
        let text = written(cents, mark, grouping);
        prop_assert_eq!(super::values::parse_csv_amount(&text, mark, eur()).map(Money::minor_units), Ok(cents));
    }

    #[test]
    fn no_file_makes_the_importer_panic(input in prop::collection::vec(any::<u8>(), 0..400)) {
        let mut settings = simple();
        settings.value_date = Some(Column::Number(3));
        settings.other_party_name = Some(Column::Number(4));
        settings.status = Some(StatusColumn { column: Column::Number(5), booked: vec!["B".to_owned()] });
        let importer = Csv::new(settings, eur()).expect("valid settings");
        let _ = importer.recognize(&input);
        let _ = import(&importer, &input, &Limits::default());
    }

    #[test]
    fn no_csv_like_file_makes_the_importer_panic(
        input in "[0-9;,.\"\\-\r\n a-zäöü€\t]{0,300}",
        mark in prop::sample::select(vec![DecimalMark::Comma, DecimalMark::Point]),
        order in prop::sample::select(vec![DateOrder::DayMonthYear, DateOrder::MonthDayYear, DateOrder::YearMonthDay]),
        skip in 0_usize..3,
    ) {
        let mut settings = CsvSettings::new(
            ';',
            mark,
            order,
            Column::Number(1),
            AmountColumns::DebitCredit { debit: Column::Number(2), credit: Column::Number(3) },
        );
        settings.header = false;
        settings.skip_lines = skip;
        settings.skip_last_lines = skip;
        settings.account = Some(Column::Number(4));
        let importer = Csv::new(settings, eur()).expect("valid settings");
        let _ = import(&importer, input.as_bytes(), &Limits::default());
    }

    #[test]
    fn no_settings_text_makes_reading_it_panic(input in "[a-z\\-=\"\\\\ 0-9\n#]{0,200}") {
        let _ = CsvSettings::from_text(&input);
    }
}
