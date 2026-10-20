// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use proptest::prelude::*;

use super::*;
use crate::import;

/// A message for the example IBAN, in euros, around `body`.
fn message(body: &str) -> String {
    format!(
        ":20:TEST\n:25:DE89370400440532013000\n:60F:C261001EUR0,00\n{body}:62F:C261031EUR0,00\n-\n"
    )
}

fn read(text: &str) -> Result<Vec<Statement>, ImportError> {
    read_with(text.as_bytes(), &Limits::default())
}

fn read_with(input: &[u8], limits: &Limits) -> Result<Vec<Statement>, ImportError> {
    let Imported::Statements(statements) = import(&Mt940, input, limits)?;
    Ok(statements)
}

/// The entries of a message holding `body`.
fn entries(body: &str) -> Vec<Entry> {
    let mut statements = read(&message(body)).expect("the message is read");
    assert_eq!(statements.len(), 1);
    statements.remove(0).entries
}

fn only_entry(body: &str) -> Entry {
    let mut entries = entries(body);
    assert_eq!(entries.len(), 1);
    entries.remove(0)
}

/// An entry with the `:86:` field `details`.
fn with_details(details: &str) -> Entry {
    only_entry(&format!(":61:2610021002C1,00NTRFNONREF\n:86:{details}\n"))
}

fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("a valid date")
}

#[test]
fn the_importer_describes_itself() {
    let info = Mt940.info();
    assert_eq!(info.id, "mt940");
    assert_eq!(info.kind, ImportKind::Statements);
    assert!(info.extensions.contains(&"sta"));
}

#[test]
fn a_statement_is_recognized_by_its_first_fields() {
    let recognize = |text: &str| Mt940.recognize(text.as_bytes());
    assert_eq!(
        recognize(":20:X\r\n:25:DE89370400440532013000\r\n:60F:C261001EUR0,00\r\n"),
        Recognition::Yes
    );
    assert_eq!(
        recognize("{1:F01TESTDEFFAXXX0000000000}{4:\n:20:X\n:60M:C261001EUR0,00\n"),
        Recognition::Yes
    );
    assert_eq!(
        recognize("\u{feff}:20:X\n:60F:C261001EUR0,00\n"),
        Recognition::Yes
    );
    // Only one of the two: the other may lie beyond the bytes looked at.
    assert_eq!(
        recognize(":20:X\n:25:DE89370400440532013000\n"),
        Recognition::Maybe
    );
    assert_eq!(recognize(":60F:C261001EUR0,00\n"), Recognition::Maybe);
    assert_eq!(recognize(""), Recognition::No);
    assert_eq!(recognize("Date;Amount\n"), Recognition::No);
    assert_eq!(recognize("<Document>"), Recognition::No);
    // A tag in the middle of a line is not a field.
    assert_eq!(recognize("a :20: b :60F: c"), Recognition::No);
}

#[test]
fn every_field_of_an_entry_is_read() {
    let entry =
        only_entry(":61:2610021003D12,50NMSCKUNDE-7//BANK-7\n:86:166?00Buch?20SVWZ+Zweck\n");
    assert_eq!(
        entry,
        Entry {
            status: EntryStatus::Booked,
            bank_id: Some("BANK-7".to_owned()),
            booking_date: date(2026, 10, 3),
            value_date: Some(date(2026, 10, 2)),
            amount: Money::from_minor_units(-1250, CurrencyCode::new("EUR").expect("a code")),
            other_party_name: None,
            other_party_iban: None,
            remittance_information: Some("Zweck".to_owned()),
        }
    );
}

#[test]
fn the_balances_and_the_account_are_read() {
    let statements = read(
        ":20:X\n:25:37040044/0532013000\n:28C:00001/001\n:60F:D261001EUR12,34\n:62F:C261031EUR0,05\n",
    )
    .expect("read");
    let statement = &statements[0];
    assert_eq!(statement.account.iban, None);
    assert_eq!(
        statement.account.number.as_deref(),
        Some("37040044/0532013000")
    );
    let opening = statement.opening_balance.expect("an opening balance");
    assert_eq!(
        (opening.amount.minor_units(), opening.date),
        (-1234, date(2026, 10, 1))
    );
    let closing = statement.closing_balance.expect("a closing balance");
    assert_eq!(
        (closing.amount.minor_units(), closing.date),
        (5, date(2026, 10, 31))
    );
}

#[test]
fn a_statement_with_only_a_closing_balance_has_a_currency() {
    let statements = read(":20:X\n:62F:C261031USD7,00\n").expect("read");
    assert_eq!(statements[0].currency.code().as_str(), "USD");
    assert_eq!(statements[0].opening_balance, None);
}

#[test]
fn a_message_ends_at_a_dash_or_at_the_next_reference() {
    let text =
        ":20:A\n:60F:C261001EUR1,00\n:20:B\n:60F:C261001EUR2,00\n-\n:20:C\n:60F:C261001EUR3,00";
    let statements = read(text).expect("read");
    let opening: Vec<i64> = statements
        .iter()
        .map(|statement| {
            statement
                .opening_balance
                .expect("balance")
                .amount
                .minor_units()
        })
        .collect();
    assert_eq!(opening, [100, 200, 300]);
}

#[test]
fn lines_may_end_with_cr_lf_and_have_empty_lines_between() {
    let text = message(":61:2610021002C1,00NTRFNONREF\n\n:86:Hello\n").replace('\n', "\r\n");
    let statements = read(&text).expect("read");
    assert_eq!(statements[0].entries.len(), 1);
    assert_eq!(
        statements[0].entries[0].remittance_information.as_deref(),
        Some("Hello")
    );
}

#[test]
fn the_direction_comes_from_the_mark() {
    let amount = |mark: &str| only_entry(&format!(":61:261002{mark}5,00NMSCNONREF\n")).amount;
    assert_eq!(amount("C").minor_units(), 500);
    assert_eq!(amount("D").minor_units(), -500);
    // The reversal of a credit takes money out, of a debit puts it back.
    assert_eq!(amount("RC").minor_units(), -500);
    assert_eq!(amount("RD").minor_units(), 500);
    // A funds code, the currency's third letter, is skipped.
    assert_eq!(amount("CR").minor_units(), 500);
    assert_eq!(amount("DR").minor_units(), -500);
    assert_eq!(amount("RDR").minor_units(), 500);
}

#[test]
fn the_booking_date_is_the_entry_date_or_else_the_value_date() {
    let dates = |line: &str| {
        let entry = only_entry(&format!("{line}\n"));
        (entry.booking_date, entry.value_date)
    };
    assert_eq!(
        dates(":61:261002C1,00NMSCNONREF"),
        (date(2026, 10, 2), Some(date(2026, 10, 2)))
    );
    assert_eq!(
        dates(":61:2610020930C1,00NMSCNONREF"),
        (date(2026, 9, 30), Some(date(2026, 10, 2)))
    );
    // The entry date has no year: the nearest to the value date counts.
    assert_eq!(
        dates(":61:2612300102C1,00NMSCNONREF"),
        (date(2027, 1, 2), Some(date(2026, 12, 30)))
    );
    assert_eq!(
        dates(":61:2701020101C1,00NMSCNONREF"),
        (date(2027, 1, 1), Some(date(2027, 1, 2)))
    );
    assert_eq!(
        dates(":61:2701020102C1,00NMSCNONREF"),
        (date(2027, 1, 2), Some(date(2027, 1, 2)))
    );
    assert_eq!(
        dates(":61:2701011231C1,00NMSCNONREF"),
        (date(2026, 12, 31), Some(date(2027, 1, 1)))
    );
    // A leap day only exists in a leap year.
    assert_eq!(
        dates(":61:2802290301C1,00NMSCNONREF"),
        (date(2028, 3, 1), Some(date(2028, 2, 29)))
    );
    assert_eq!(
        dates(":61:2703010229C1,00NMSCNONREF"),
        (date(2028, 2, 29), Some(date(2027, 3, 1)))
    );
}

#[test]
fn two_digit_years_follow_posix() {
    let value_date = |line: &str| only_entry(&format!("{line}\n")).value_date;
    assert_eq!(
        value_date(":61:680101C1,00NMSCNONREF"),
        Some(date(2068, 1, 1))
    );
    assert_eq!(
        value_date(":61:690101C1,00NMSCNONREF"),
        Some(date(1969, 1, 1))
    );
    assert_eq!(
        value_date(":61:991231C1,00NMSCNONREF"),
        Some(date(1999, 12, 31))
    );
    assert_eq!(
        value_date(":61:000101C1,00NMSCNONREF"),
        Some(date(2000, 1, 1))
    );
}

#[test]
fn the_bank_reference_is_the_bank_id() {
    let id = |line: &str| only_entry(&format!("{line}\n")).bank_id;
    assert_eq!(id(":61:261002C1,00NMSCCUST//BANK"), Some("BANK".to_owned()));
    // The customer's reference is never the ID: it repeats.
    assert_eq!(id(":61:261002C1,00NMSCCUST"), None);
    assert_eq!(id(":61:261002C1,00NMSC"), None);
    assert_eq!(id(":61:261002C1,00NMSCNONREF//NONREF"), None);
    assert_eq!(id(":61:261002C1,00NMSC//nonref"), None);
    assert_eq!(id(":61:261002C1,00NMSC//NOTPROVIDED"), None);
    assert_eq!(id(":61:261002C1,00NMSC//"), None);
    assert_eq!(id(":61:261002C1,00NMSC//  "), None);
    assert_eq!(id(":61:261002C1,00NMSCA/B//C/D"), Some("C/D".to_owned()));
}

#[test]
fn the_line_below_an_entry_is_not_part_of_its_text() {
    let entry = only_entry(":61:261002C1,00NMSCNONREF\nEUR1,00 supplementary\n:86:Note\n");
    assert_eq!(entry.remittance_information.as_deref(), Some("Note"));
}

#[test]
fn an_entry_without_notes_has_none() {
    let entry = only_entry(":61:261002C1,00NMSCNONREF\n");
    assert_eq!(entry.other_party_name, None);
    assert_eq!(entry.other_party_iban, None);
    assert_eq!(entry.remittance_information, None);
}

#[test]
fn notes_about_the_statement_belong_to_no_entry() {
    let entries =
        entries(":61:261002C1,00NMSCNONREF\n:64:C261002EUR1,00\n:86:About the statement\n");
    assert_eq!(entries[0].remittance_information, None);
}

#[test]
fn free_text_keeps_its_lines() {
    let entry = with_details("Line one\nLine two\n   \nLine three");
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("Line one\nLine two\nLine three")
    );
    assert_eq!(entry.other_party_name, None);
}

#[test]
fn a_question_mark_is_text_unless_two_digits_follow() {
    let entry = with_details("Why?\nBecause ?x and ?5.");
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("Why?\nBecause ?x and ?5.")
    );
}

#[test]
fn a_structured_note_may_leave_out_the_booking_key() {
    let entry = with_details("?00Text?20Purpose?32Name");
    assert_eq!(entry.remittance_information.as_deref(), Some("Purpose"));
    assert_eq!(entry.other_party_name.as_deref(), Some("Name"));
    // Something else in front is not the structured form.
    let entry = with_details("Hello ?20Purpose");
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("Hello ?20Purpose")
    );
}

#[test]
fn a_structured_note_is_joined_across_lines_without_a_separator() {
    let entry = with_details(
        "166?00Gutschrift?20SVWZ+Rechnung 12\n345 vom 1.10.?\n21Danke?32Max\n Mustermann",
    );
    // "?\n21" is joined to "?21", a sub-field of its own.
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("Rechnung 12345 vom 1.10.Danke")
    );
    assert_eq!(entry.other_party_name.as_deref(), Some("Max Mustermann"));
}

#[test]
fn the_purpose_is_every_purpose_sub_field_in_order() {
    let entry =
        with_details("000?20a?21b?22c?23d?24e?25f?26g?27h?28i?29j?60k?61l?62m?63n?30x?31y?33z");
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("abcdefghijklmn")
    );
}

#[test]
fn tags_split_the_purpose_only_where_a_part_starts() {
    // Not at the start of a sub-field or after a space: just text.
    let entry = with_details("166?20Re:SVWZ+ not a tag in AB?21CSVWZ+nor here");
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("Re:SVWZ+ not a tag in ABCSVWZ+nor here")
    );
    // A tag after a space, or at the start of a sub-field, is one.
    let entry = with_details("166?20EREF+1 SVWZ+The text?22IBAN+DE89370400440532013000");
    assert_eq!(entry.remittance_information.as_deref(), Some("The text"));
    assert_eq!(
        entry.other_party_iban.as_deref(),
        Some("DE89370400440532013000")
    );
}

#[test]
fn a_purpose_without_the_payers_text_keeps_what_stands_before_the_tags() {
    let entry = with_details("166?00Booking?20Free text EREF+1 KREF+2");
    assert_eq!(entry.remittance_information.as_deref(), Some("Free text"));
    let entry = with_details("166?00Booking?20EREF+1 KREF+2");
    assert_eq!(entry.remittance_information.as_deref(), Some("Booking"));
}

#[test]
fn the_text_of_the_booking_stands_in_for_a_missing_purpose() {
    let entry = with_details("808?00Entgelt?20   ");
    assert_eq!(entry.remittance_information.as_deref(), Some("Entgelt"));
    let entry = with_details("808?10931");
    assert_eq!(entry.remittance_information, None);
}

#[test]
fn the_other_partys_account_is_chosen_from_what_the_note_gives() {
    let iban = |details: &str| with_details(details).other_party_iban;
    // An IBAN in the account sub-field wins over a tag.
    assert_eq!(
        iban("001?20IBAN+GB82WEST12345698765432?31DE89370400440532013000"),
        Some("DE89370400440532013000".to_owned())
    );
    // The tag when the sub-field has none, or has no IBAN.
    assert_eq!(
        iban("001?20IBAN+GB82WEST12345698765432"),
        Some("GB82WEST12345698765432".to_owned())
    );
    assert_eq!(
        iban("001?20IBAN+GB82WEST12345698765432?31 12345 ?30 678 "),
        Some("GB82WEST12345698765432".to_owned())
    );
    // A bank code and an account number, as the statement's own account.
    assert_eq!(
        iban("001?30 37040044?31 0532013000 "),
        Some("37040044/0532013000".to_owned())
    );
    // An account number alone, or with a BIC, is kept as it is.
    assert_eq!(iban("001?31 0532013000"), Some("0532013000".to_owned()));
    assert_eq!(
        iban("001?30COBADEFFXXX?31 0532013000"),
        Some("0532013000".to_owned())
    );
    assert_eq!(iban("001?30COBADEFFXXX"), None);
    assert_eq!(iban("001?31   "), None);
}

#[test]
fn the_name_is_both_name_sub_fields() {
    assert_eq!(
        with_details("001?32First part?33 and second")
            .other_party_name
            .as_deref(),
        Some("First part and second")
    );
    assert_eq!(
        with_details("001?33Only second")
            .other_party_name
            .as_deref(),
        Some("Only second")
    );
    assert_eq!(with_details("001?32   ?33").other_party_name, None);
}

#[test]
fn text_is_kept_as_delivered() {
    let entry = with_details("001?20SVWZ+  padded  ?32  Name  ");
    assert_eq!(entry.remittance_information.as_deref(), Some("  padded  "));
    assert_eq!(entry.other_party_name.as_deref(), Some("  Name  "));
}

#[test]
fn a_byte_order_mark_and_umlauts_in_either_encoding_are_read() {
    let utf8 = format!(
        "\u{feff}{}",
        message(":61:261002C1,00NMSCNONREF\n:86:?20Grüße\n")
    );
    let statements = read_with(utf8.as_bytes(), &Limits::default()).expect("UTF-8");
    assert_eq!(
        statements[0].entries[0].remittance_information.as_deref(),
        Some("Grüße")
    );
    // The same in Windows-1252, where ü and ß are single bytes.
    let mut input = b":20:X\n:60F:C261001EUR0,00\n:61:261002C1,00NMSCNONREF\n:86:?20Gr".to_vec();
    input.extend([0xFC, 0xDF, b'e', b'\n', b'-', b'\n']);
    let statements = read_with(&input, &Limits::default()).expect("Windows-1252");
    assert_eq!(
        statements[0].entries[0].remittance_information.as_deref(),
        Some("Grüße")
    );
}

#[test]
fn a_byte_no_encoding_has_is_malformed_on_its_line() {
    let input = b":20:X\n:60F:C261001EUR0,00\n:86:\x81\n";
    assert_eq!(
        read_with(input, &Limits::default()),
        Err(ImportError::Malformed { line: Some(3) })
    );
    assert_eq!(
        read_with(b":20:X\n:60F:C261001EUR0,00\n\0\n", &Limits::default()),
        Err(ImportError::Malformed { line: Some(3) })
    );
}

#[test]
fn amounts_are_digits_and_a_comma() {
    let amount = |text: &str| {
        let line = format!(":61:261002C{text}NMSCNONREF\n");
        read(&message(&line))
            .map(|mut statements| statements.remove(0).entries.remove(0).amount.minor_units())
    };
    assert_eq!(amount("12,50"), Ok(1250));
    assert_eq!(amount("12,5"), Ok(1250));
    assert_eq!(amount("12,"), Ok(1200));
    assert_eq!(amount("0,01"), Ok(1));
    assert_eq!(amount("0,00"), Ok(0));
    assert_eq!(amount("1234567,89"), Ok(123_456_789));
    for bad in [
        "12.50", "12", ",50", "-1,00", "+1,00", "1,000", "1,2,3", "1.234,50", ",",
    ] {
        let result = amount(bad);
        assert!(
            matches!(
                result,
                Err(ImportError::BadAmount { .. } | ImportError::TooManyDecimalPlaces { .. })
            ),
            "{bad} gave {result:?}"
        );
    }
    assert!(matches!(
        amount("99999999999999999999,99"),
        Err(ImportError::AmountTooLarge { .. })
    ));
}

#[test]
fn the_smallest_and_largest_amounts_arrive() {
    let balance = |mark: &str, amount: &str| {
        let text = format!(":20:X\n:60F:{mark}261001EUR{amount}\n-\n");
        read(&text).map(|statements| {
            statements[0]
                .opening_balance
                .expect("balance")
                .amount
                .minor_units()
        })
    };
    assert_eq!(balance("D", "92233720368547758,08"), Ok(i64::MIN));
    assert_eq!(balance("C", "92233720368547758,07"), Ok(i64::MAX));
    assert!(matches!(
        balance("C", "92233720368547758,08"),
        Err(ImportError::AmountTooLarge { .. })
    ));
}

#[test]
fn a_whole_currency_has_no_decimal_places() {
    let text = ":20:X\n:60F:C261001JPY100,\n:61:261002D5,NMSCNONREF\n:62F:C261002JPY95,\n";
    let statements = read(text).expect("read");
    assert_eq!(statements[0].currency.minor_digits(), 0);
    assert_eq!(statements[0].entries[0].amount.minor_units(), -5);
    assert!(matches!(
        read(":20:X\n:60F:C261001JPY1,5\n"),
        Err(ImportError::TooManyDecimalPlaces { .. })
    ));
}

#[test]
fn errors_say_where_they_are() {
    let line_of = |text: &str| match read(text) {
        Err(ImportError::BadAmount { line, .. }) => line,
        other => panic!("expected a bad amount, got {other:?}"),
    };
    assert_eq!(line_of(&message(":61:261002C1.00NMSCNONREF\n")), Some(4));
    assert_eq!(line_of(":20:X\n\n:60F:C261001EUR1.0\n"), Some(3));
    // The line counts from the start of the file, also behind a SWIFT block.
    assert_eq!(
        line_of("{1:F01X}{4:\n:20:X\n:60F:C261001EUR0,00\n:61:261002C1.00NMSCNONREF\n-}\n"),
        Some(4)
    );
}

#[test]
fn a_file_that_is_no_statement_is_malformed() {
    for text in [
        "", "\n\n", "{1:X}\n", "hello\n", ":20:X\n", "-\n", ":99:x\n",
    ] {
        assert!(
            matches!(read(text), Err(ImportError::Malformed { .. })),
            "{text:?}"
        );
    }
}

#[test]
fn the_limits_are_kept() {
    let limits = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    let entries = ":61:261002C1,00NMSCNONREF\n".repeat(3);
    assert_eq!(
        read_with(message(&entries).as_bytes(), &limits),
        Err(ImportError::TooManyEntries { max: 2 })
    );
    // Entries of all statements count.
    let two_messages = format!(
        "{}{}",
        message(":61:261002C1,00NMSCNONREF\n:61:261002C1,00NMSCNONREF\n"),
        message(":61:261002C1,00NMSCNONREF\n")
    );
    assert_eq!(
        read_with(two_messages.as_bytes(), &limits),
        Err(ImportError::TooManyEntries { max: 2 })
    );

    let limits = Limits {
        max_text_bytes: 30,
        ..Limits::default()
    };
    let long = message(":61:261002C1,00NMSCNONREF\n:86:Some text\nthat is far too long indeed\n");
    assert_eq!(
        read_with(long.as_bytes(), &limits),
        Err(ImportError::TextTooLong {
            max_bytes: 30,
            line: Some(5)
        })
    );
    let long_id =
        message(":61:261002C1,00NMSCNONREF//A-VERY-LONG-BANK-REFERENCE-THAT-IS-OVER-THE-LIMIT\n");
    assert_eq!(
        read_with(long_id.as_bytes(), &limits),
        Err(ImportError::TextTooLong {
            max_bytes: 30,
            line: Some(4)
        })
    );
}

#[test]
fn fields_the_importer_does_not_read_are_not_kept() {
    // A file of nothing but such fields must not cost memory in proportion
    // to their number; here it is only that they are skipped, and fast.
    let text = format!(
        ":20:X\n:60F:C261001EUR0,00\n{}:62F:C261031EUR0,00\n-\n",
        ":99:x\ncontinued\n".repeat(200_000)
    );
    let statements = read(&text).expect("read");
    assert!(statements[0].entries.is_empty());
    // A message that has only such fields has no balance.
    assert_eq!(
        read(":20:A\n:20:B\n:13D:x\n"),
        Err(ImportError::Malformed { line: Some(1) })
    );
}

#[test]
fn a_message_has_no_more_than_a_few_fields_besides_entries() {
    let text = format!(":20:X\n{}", ":25:DE89370400440532013000\n".repeat(9));
    assert_eq!(read(&text), Err(ImportError::Malformed { line: Some(10) }));
}

#[test]
fn a_note_belongs_to_its_entry_only_when_it_comes_right_after() {
    let entry =
        only_entry(":61:261002C1,00NMSCNONREF\n:13D:2610011200+0200\n:86:Not for the entry\n");
    assert_eq!(entry.remittance_information, None);
}

#[test]
fn many_sub_fields_are_read_in_linear_time() {
    // A note of nothing but sub-field markers, as a hostile file might
    // have: this must not take longer than the note is long.
    let note = "?20".repeat(5000);
    let entry = with_details(&note);
    assert_eq!(entry.remittance_information, None);
    let tags = "?20 EREF+".repeat(1500);
    let entry = with_details(&tags);
    assert_eq!(entry.remittance_information, None);
}

proptest! {
    /// Every digit of an amount arrives, with the sign from the mark.
    #[test]
    fn amounts_are_exact(minor in 0_i64..=i64::MAX / 2, debit in any::<bool>()) {
        let text = format!("{},{:02}", minor / 100, minor % 100);
        let mark = if debit { "D" } else { "C" };
        let entry = only_entry(&format!(":61:261002{mark}{text}NMSCNONREF\n"));
        let expected = if debit { -minor } else { minor };
        prop_assert_eq!(entry.amount.minor_units(), expected);
    }

    /// The booking date is the entry date's day in the year that puts it
    /// nearest to the value date, so never more than half a year away.
    #[test]
    fn the_booking_date_is_near_the_value_date(
        year in 1970_i16..=2060,
        value_month in 1_i8..=12,
        value_day in 1_i8..=28,
        month in 1_i8..=12,
        day in 1_i8..=28,
    ) {
        let value = date(year, value_month, value_day);
        let booking = booking_date(value, &format!("{month:02}{day:02}")).expect("a date");
        prop_assert_eq!((booking.month(), booking.day()), (month, day));
        let months = (i32::from(booking.year()) - i32::from(year)) * 12
            + i32::from(booking.month())
            - i32::from(value_month);
        prop_assert!(months.abs() <= 6, "{value} and {booking} are {months} months apart");
    }

    /// No input makes the importer panic, whatever it reads.
    #[test]
    fn any_bytes_give_a_result_or_an_error(input in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = read_with(&input, &Limits::default());
        let _ = Mt940.recognize(&input);
    }

    /// Nor does text made of the importer's own punctuation.
    #[test]
    fn text_of_tags_and_markers_gives_a_result_or_an_error(
        parts in proptest::collection::vec(
            prop_oneof![
                Just(":20:".to_owned()),
                Just(":25:".to_owned()),
                Just(":60F:".to_owned()),
                Just(":61:".to_owned()),
                Just(":62F:".to_owned()),
                Just(":86:".to_owned()),
                Just("\n".to_owned()),
                Just("-\n".to_owned()),
                Just("?20".to_owned()),
                Just("?32".to_owned()),
                Just("SVWZ+".to_owned()),
                Just("IBAN+".to_owned()),
                Just("C261001EUR0,00".to_owned()),
                Just("261002D1,00NMSCNONREF//X".to_owned()),
                Just("Grüße".to_owned()),
                "[ -~äöü]{0,12}",
            ],
            0..40,
        )
    ) {
        let _ = read(&parts.concat());
    }

    /// Nor does a statement that is cut off or has a byte changed.
    #[test]
    fn a_damaged_statement_gives_a_result_or_an_error(cut in 0_usize..500, at in 0_usize..500, byte in any::<u8>()) {
        let full = message(
            ":61:2610021003D12,50NMSCKUNDE-7//BANK-7\n:86:166?00Buch?20SVWZ+Zweck?31DE89370400440532013000?32Name\n",
        )
        .into_bytes();
        let mut damaged = full[..cut.min(full.len())].to_vec();
        if let Some(slot) = damaged.get_mut(at) {
            *slot = byte;
        }
        let _ = read_with(&damaged, &Limits::default());
    }
}
