// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use proptest::prelude::*;

use super::*;
use crate::import;
use crate::statement::EntryStatus;

/// An OFX 1.x file for the example IBAN, in euros, around `body`.
fn file(body: &str) -> String {
    format!(
        "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\nSECURITY:NONE\nENCODING:USASCII\n\
         CHARSET:1252\nCOMPRESSION:NONE\nOLDFILEUID:NONE\nNEWFILEUID:NONE\n\n\
         <OFX>\n<BANKMSGSRSV1>\n<STMTTRNRS>\n<TRNUID>1\n<STMTRS>\n<CURDEF>EUR\n\
         <BANKACCTFROM>\n<BANKID>COBADEFF\n<ACCTID>DE89370400440532013000\n\
         <ACCTTYPE>CHECKING\n</BANKACCTFROM>\n<BANKTRANLIST>\n{body}</BANKTRANLIST>\n\
         <LEDGERBAL><BALAMT>10.00<DTASOF>20261031</LEDGERBAL>\n\
         </STMTRS>\n</STMTTRNRS>\n</BANKMSGSRSV1>\n</OFX>\n"
    )
}

/// An entry of `amount` on 2 October 2026, with `rest` after its date.
fn transaction(amount: &str, rest: &str) -> String {
    format!("<STMTTRN>\n<DTPOSTED>20261002\n<TRNAMT>{amount}\n{rest}</STMTTRN>\n")
}

fn read(text: &str) -> Result<Vec<Statement>, ImportError> {
    read_with(text.as_bytes(), &Limits::default())
}

fn read_with(input: &[u8], limits: &Limits) -> Result<Vec<Statement>, ImportError> {
    match import(&Ofx, input, limits)? {
        Imported::Statements(statements) => Ok(statements),
        other => panic!("a statement importer read {other:?}"),
    }
}

fn entries(body: &str) -> Vec<Entry> {
    let mut statements = read(&file(body)).expect("the file is read");
    assert_eq!(statements.len(), 1);
    statements.remove(0).entries
}

fn only_entry(body: &str) -> Entry {
    let mut entries = entries(body);
    assert_eq!(entries.len(), 1);
    entries.remove(0)
}

fn date(year: i16, month: i8, day: i8) -> Date {
    Date::new(year, month, day).expect("a valid date")
}

fn malformed(line: u64) -> ImportError {
    ImportError::Malformed { line: Some(line) }
}

#[test]
fn the_importer_describes_itself() {
    let info = Ofx.info();
    assert_eq!(info.id, "ofx");
    assert_eq!(info.kind, ImportKind::Statements);
    assert!(info.extensions.contains(&"ofx") && info.extensions.contains(&"qfx"));
}

#[test]
fn a_file_is_recognized_by_its_header_or_its_root() {
    let recognize = |text: &str| Ofx.recognize(text.as_bytes());
    assert_eq!(
        recognize("OFXHEADER:100\r\nDATA:OFXSGML\r\n"),
        Recognition::Yes
    );
    assert_eq!(recognize("\n<OFX>\n<SIGNONMSGSRSV1>"), Recognition::Yes);
    assert_eq!(
        recognize("<?xml version=\"1.0\"?>\n<?OFX OFXHEADER=\"200\"?>\n"),
        Recognition::Yes
    );
    assert_eq!(recognize("<ofx>"), Recognition::Yes);
    assert_eq!(recognize("\u{feff}OFXHEADER:100"), Recognition::Yes);
    assert_eq!(recognize(""), Recognition::No);
    assert_eq!(recognize("Date;Amount\n"), Recognition::No);
    assert_eq!(recognize(":20:X\n:60F:C261001EUR0,00\n"), Recognition::No);
    assert_eq!(recognize("<Document>"), Recognition::No);
}

#[test]
fn every_field_of_an_entry_is_read() {
    let entry = only_entry(
        "<STMTTRN>\n<TRNTYPE>DEBIT\n<DTPOSTED>20261002120000\n<TRNAMT>-25.50\n<FITID>FIT-1\n\
         <NAME>Muster Energie AG\n<MEMO>Stromabschlag\n\
         <BANKACCTTO>\n<ACCTID>DE02120300000000202051\n</BANKACCTTO>\n</STMTTRN>\n",
    );
    let eur = CurrencyCode::new("EUR").expect("a code");
    assert_eq!(entry.status, EntryStatus::Booked);
    assert_eq!(entry.bank_id.as_deref(), Some("FIT-1"));
    assert_eq!(entry.booking_date, date(2026, 10, 2));
    assert_eq!(entry.value_date, None);
    assert_eq!(entry.amount, Money::from_minor_units(-2550, eur));
    assert_eq!(entry.other_party_name.as_deref(), Some("Muster Energie AG"));
    assert_eq!(
        entry.other_party_iban.as_deref(),
        Some("DE02120300000000202051")
    );
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("Stromabschlag")
    );
}

#[test]
fn the_account_balance_and_currency_are_read() {
    let statements = read(&file("")).expect("read");
    let statement = &statements[0];
    assert_eq!(
        statement.account.iban.as_deref(),
        Some("DE89370400440532013000")
    );
    assert_eq!(statement.account.number, None);
    assert_eq!(statement.currency.code().to_string(), "EUR");
    assert_eq!(statement.opening_balance, None);
    let closing = statement.closing_balance.expect("a closing balance");
    assert_eq!(closing.amount.minor_units(), 1000);
    assert_eq!(closing.date, date(2026, 10, 31));
}

#[test]
fn an_account_that_is_no_iban_is_named_by_bank_code_and_number() {
    let text = file("").replace("DE89370400440532013000", "0532013000");
    let account = read(&text).expect("read").remove(0).account;
    assert_eq!(account.iban, None);
    assert_eq!(account.number.as_deref(), Some("COBADEFF/0532013000"));
    let text = text.replace("<BANKID>COBADEFF\n", "");
    let account = read(&text).expect("read").remove(0).account;
    assert_eq!(account.number.as_deref(), Some("0532013000"));
}

#[test]
fn money_in_is_positive_and_money_out_negative_whatever_the_type() {
    let amount = |text: &str, kind: &str| {
        only_entry(&transaction(text, &format!("<TRNTYPE>{kind}\n")))
            .amount
            .minor_units()
    };
    assert_eq!(amount("12.50", "CREDIT"), 1250);
    assert_eq!(amount("-12.50", "DEBIT"), -1250);
    // The amount's sign decides, not the type.
    assert_eq!(amount("12.50", "DEBIT"), 1250);
    assert_eq!(amount("-12.50", "CREDIT"), -1250);
    assert_eq!(amount("+12.50", "CREDIT"), 1250);
}

#[test]
fn amounts_have_a_point_or_a_comma_and_nothing_else() {
    let amount = |text: &str| {
        read(&file(&transaction(text, "")))
            .map(|mut statements| statements.remove(0).entries.remove(0).amount.minor_units())
    };
    assert_eq!(amount("7"), Ok(700));
    assert_eq!(amount("7.5"), Ok(750));
    assert_eq!(amount("0.05"), Ok(5));
    assert_eq!(amount("12,50"), Ok(1250));
    assert_eq!(amount("-0,99"), Ok(-99));
    for bad in [
        "1,234.50", "1.234,50", "12.5.0", ".50", "1 000.00", "1e3", "x", "--1",
    ] {
        assert!(
            matches!(amount(bad), Err(ImportError::BadAmount { .. })),
            "{bad}: {:?}",
            amount(bad)
        );
    }
    // A comma is a decimal mark, so "1,234" is never one thousand and more.
    assert!(matches!(
        amount("1,234"),
        Err(ImportError::TooManyDecimalPlaces { .. })
    ));
    assert!(matches!(
        amount("-1.500"),
        Err(ImportError::TooManyDecimalPlaces { .. })
    ));
    assert!(matches!(
        amount("92233720368547758.08"),
        Err(ImportError::AmountTooLarge { .. })
    ));
}

#[test]
fn dates_are_the_calendar_day_as_written() {
    let posted = |text: &str| {
        read(&file(&format!(
            "<STMTTRN>\n<DTPOSTED>{text}\n<TRNAMT>1.00\n</STMTTRN>\n"
        )))
        .map(|mut statements| statements.remove(0).entries.remove(0).booking_date)
    };
    let day = date(2026, 10, 2);
    for good in [
        "20261002",
        "202610021430",
        "20261002235959",
        "20261002000000.123",
        "20261002120000[-5:EST]",
        "20261002120000.000[0:GMT]",
        "20261002[+1:CET]",
    ] {
        assert_eq!(posted(good), Ok(day), "{good}");
    }
    for bad in [
        "20261301",
        "20260230",
        "2026-10-02",
        "261002",
        "2026100212",
        "20261002120000.",
        "20261002120000x",
        "20261002120000[",
        "1234567890123",
        "00000000",
    ] {
        assert!(
            matches!(posted(bad), Err(ImportError::BadDate { .. })),
            "{bad}: {:?}",
            posted(bad)
        );
    }
}

#[test]
fn the_name_is_the_name_or_else_the_payees() {
    let name = |rest: &str| only_entry(&transaction("-1.00", rest)).other_party_name;
    assert_eq!(name("<NAME>Direct\n").as_deref(), Some("Direct"));
    assert_eq!(
        name("<PAYEE>\n<NAME>Payee\n<ADDR1>Street 1\n</PAYEE>\n").as_deref(),
        Some("Payee")
    );
    assert_eq!(
        name("<NAME>Direct\n<PAYEE>\n<NAME>Payee\n</PAYEE>\n").as_deref(),
        Some("Direct")
    );
    assert_eq!(name(""), None);
}

#[test]
fn the_other_partys_account_is_an_iban_or_bank_code_and_number() {
    let iban = |rest: &str| only_entry(&transaction("-1.00", rest)).other_party_iban;
    assert_eq!(
        iban("<BANKACCTTO>\n<BANKID>COBADEFF\n<ACCTID>DE02120300000000202051\n</BANKACCTTO>\n")
            .as_deref(),
        Some("DE02120300000000202051")
    );
    assert_eq!(
        iban("<BANKACCTTO>\n<BANKID>12030000\n<ACCTID>202051\n</BANKACCTTO>\n").as_deref(),
        Some("12030000/202051")
    );
    assert_eq!(
        iban("<CCACCTTO>\n<ACCTID>4111111111111111\n</CCACCTTO>\n").as_deref(),
        Some("4111111111111111")
    );
    assert_eq!(iban(""), None);
}

#[test]
fn the_transaction_id_is_the_bank_id_unless_it_says_none() {
    let id = |value: &str| only_entry(&transaction("-1.00", &format!("<FITID>{value}\n"))).bank_id;
    assert_eq!(id("A-1").as_deref(), Some("A-1"));
    assert_eq!(id("NONREF"), None);
    assert_eq!(id("notprovided"), None);
    assert_eq!(id("  "), None);
    assert_eq!(only_entry(&transaction("-1.00", "")).bank_id, None);
}

#[test]
fn text_is_kept_as_delivered_with_entities_replaced() {
    let name = |value: &str| {
        let text = file(&transaction("-1.00", &format!("<NAME>{value}\n")))
            .replace("CHARSET:1252", "CHARSET:NONE");
        read(&text)
            .expect("read")
            .remove(0)
            .entries
            .remove(0)
            .other_party_name
            .expect("a name")
    };
    assert_eq!(
        name("AT&amp;T &lt;x&gt; &quot;q&quot; &apos;a&apos; &#65;&#x42;"),
        "AT&T <x> \"q\" 'a' AB"
    );
    // A `&` that starts no entity is the character itself.
    assert_eq!(name("Smith & Sons &amp"), "Smith & Sons &amp");
    assert_eq!(name("&unknown; &; &#; &#x;"), "&unknown; &; &#; &#x;");
    assert_eq!(name("a\u{202e}b\u{a0}c"), "a\u{202e}b\u{a0}c");
    // `&amp;lt;` is `&lt;` as text, not `<`.
    assert_eq!(name("&amp;lt;"), "&lt;");
}

#[test]
fn a_character_reference_must_be_a_character() {
    for bad in ["&#0;", "&#1114112;", "&#xD800;", "&#x0;"] {
        let text = file(&transaction("-1.00", &format!("<NAME>{bad}\n")));
        assert!(
            matches!(read(&text), Err(ImportError::Malformed { .. })),
            "{bad}"
        );
    }
}

#[test]
fn xml_and_sgml_give_the_same_statement() {
    let sgml = file(&transaction("-1.00", "<FITID>1\n<NAME>A\n<MEMO>B\n"));
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<?OFX OFXHEADER=\"200\"?>\n<OFX>\
         <BANKMSGSRSV1><STMTTRNRS><TRNUID>1</TRNUID><STMTRS><CURDEF>EUR</CURDEF>\
         <BANKACCTFROM><BANKID>COBADEFF</BANKID><ACCTID>DE89370400440532013000</ACCTID>\
         <ACCTTYPE>CHECKING</ACCTTYPE></BANKACCTFROM><BANKTRANLIST>\
         <STMTTRN><DTPOSTED>20261002</DTPOSTED><TRNAMT>-1.00</TRNAMT><FITID>1</FITID>\
         <NAME>A</NAME><MEMO>B</MEMO></STMTTRN></BANKTRANLIST>\
         <LEDGERBAL><BALAMT>10.00</BALAMT><DTASOF>20261031</DTASOF></LEDGERBAL>\
         </STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";
    assert_eq!(read(&sgml), read(xml));
}

#[test]
fn an_empty_element_has_no_value() {
    // `<NAME/>` and `<MEMO></MEMO>` in XML; `<MEMO>` with no end tag, right
    // before another element's, in SGML.
    for rest in [
        "<NAME/>\n<MEMO></MEMO>\n",
        "<NAME>\n<MEMO>\n",
        "<NAME>   \n<MEMO>\n",
    ] {
        let entry = only_entry(&transaction("-1.00", rest));
        assert_eq!(entry.other_party_name, None, "{rest}");
        assert_eq!(entry.remittance_information, None, "{rest}");
    }
}

#[test]
fn tags_may_be_in_any_case() {
    let mut tags_only = String::new();
    let mut inside = false;
    for character in file(&transaction("-1.00", "<NAME>Name\n")).chars() {
        inside = (inside || character == '<') && character != '>';
        tags_only.push(if inside || character == '>' {
            character.to_ascii_lowercase()
        } else {
            character
        });
    }
    // Only the tags are in lower case; the text keeps its case.
    let entry = read(&tags_only).expect("read").remove(0).entries.remove(0);
    assert_eq!(entry.other_party_name.as_deref(), Some("Name"));
}

#[test]
fn several_statements_in_one_file_stay_apart() {
    let one = "<STMTTRNRS><STMTRS><CURDEF>EUR</CURDEF><BANKACCTFROM><ACCTID>A</ACCTID></BANKACCTFROM>\
               <BANKTRANLIST><STMTTRN><DTPOSTED>20261002</DTPOSTED><TRNAMT>1.00</TRNAMT></STMTTRN>\
               </BANKTRANLIST></STMTRS></STMTTRNRS>";
    let text = format!("<OFX><BANKMSGSRSV1>{one}{one}</BANKMSGSRSV1></OFX>");
    let statements = read(&text).expect("read");
    assert_eq!(statements.len(), 2);
    assert!(
        statements
            .iter()
            .all(|statement| statement.entries.len() == 1)
    );
}

#[test]
fn what_the_importer_does_not_read_is_skipped() {
    let text = file("").replace(
        "</BANKMSGSRSV1>",
        "</BANKMSGSRSV1><INVSTMTMSGSRSV1><INVSTMTTRNRS><INVSTMTRS><CURDEF>USD</CURDEF>\
         <INVTRANLIST><STMTTRN><TRNAMT>x</TRNAMT></STMTTRN></INVTRANLIST>\
         </INVSTMTRS></INVSTMTTRNRS></INVSTMTMSGSRSV1>",
    );
    assert_eq!(read(&text).expect("read").len(), 1);
}

#[test]
fn an_entry_in_another_currency_is_refused() {
    let eur = CurrencyCode::new("EUR").expect("a code");
    let usd = CurrencyCode::new("USD").expect("a code");
    for tag in ["CURRENCY", "ORIGCURRENCY"] {
        let rest = format!("<{tag}>\n<CURRATE>1.1\n<CURSYM>USD\n</{tag}>\n");
        assert_eq!(
            read(&file(&transaction("-1.00", &rest))),
            Err(ImportError::CurrencyMismatch {
                expected: eur,
                found: usd
            })
        );
        let rest = format!("<{tag}>\n<CURRATE>1.0\n<CURSYM>EUR\n</{tag}>\n");
        assert!(read(&file(&transaction("-1.00", &rest))).is_ok());
    }
}

#[test]
fn a_correction_is_refused() {
    let rest = "<CORRECTFITID>OLD\n<CORRECTACTION>REPLACE\n";
    assert!(matches!(
        read(&file(&transaction("-1.00", rest))),
        Err(ImportError::Malformed { line: Some(_) })
    ));
    // An empty reference corrects nothing.
    assert!(read(&file(&transaction("-1.00", "<CORRECTFITID/>\n"))).is_ok());
}

#[test]
fn a_whole_currency_has_no_decimal_places() {
    let text = file(&transaction("-1500", ""))
        .replace("EUR", "JPY")
        .replace("10.00", "10");
    let statement = read(&text).expect("read").remove(0);
    assert_eq!(statement.entries[0].amount.minor_units(), -1500);
    let text = file(&transaction("-1500.5", "")).replace("EUR", "JPY");
    assert!(matches!(
        read(&text),
        Err(ImportError::TooManyDecimalPlaces { .. })
    ));
}

#[test]
fn a_withdrawn_or_unknown_currency_is_refused_on_its_line() {
    for code in ["DEM", "XYZ"] {
        let found = CurrencyCode::new(code).expect("a code");
        assert_eq!(
            read(&file("").replace("EUR", code)),
            Err(ImportError::UnknownCurrency {
                currency: found,
                line: Some(16)
            })
        );
    }
    assert_eq!(
        read(&file("").replace("<CURDEF>EUR\n", "")),
        Err(malformed(15))
    );
    assert_eq!(read(&file("").replace("EUR", "EU")), Err(malformed(16)));
}

#[test]
fn an_entry_needs_its_amount_and_its_date() {
    assert_eq!(
        read(&file("<STMTTRN>\n<DTPOSTED>20261002\n</STMTTRN>\n")),
        Err(malformed(23))
    );
    assert_eq!(
        read(&file("<STMTTRN>\n<TRNAMT>1.00\n</STMTTRN>\n")),
        Err(malformed(23))
    );
}

#[test]
fn a_balance_needs_its_amount_and_its_day() {
    let text = file("").replace("<DTASOF>20261031", "");
    assert!(matches!(
        read(&text),
        Err(ImportError::Malformed { line: Some(_) })
    ));
    let text = file("").replace("<BALAMT>10.00", "");
    assert!(matches!(
        read(&text),
        Err(ImportError::Malformed { line: Some(_) })
    ));
    let text = file("").replace("<LEDGERBAL><BALAMT>10.00<DTASOF>20261031</LEDGERBAL>\n", "");
    assert_eq!(read(&text).expect("read")[0].closing_balance, None);
}

#[test]
fn errors_say_where_they_are() {
    let text = file(&transaction("oops", ""));
    assert_eq!(
        read(&text),
        Err(ImportError::BadAmount {
            found: "oops".to_owned(),
            line: Some(25)
        })
    );
}

#[test]
fn encodings_follow_the_header() {
    let text = file(&transaction("-1.00", "<NAME>Bäckerei €\n"));
    let bytes: Vec<u8> = text
        .chars()
        .map(|c| match c {
            'ä' => 0xE4,
            '€' => 0x80,
            c => u8::try_from(u32::from(c)).expect("ascii"),
        })
        .collect();
    let name = |input: &[u8]| {
        read_with(input, &Limits::default())
            .map(|mut s| s.remove(0).entries.remove(0).other_party_name)
    };
    // CHARSET:1252
    assert_eq!(name(&bytes), Ok(Some("Bäckerei €".to_owned())));
    // UTF-8 is read as such when the header says it, and when it says nothing.
    let utf8 = text
        .replace("CHARSET:1252", "CHARSET:NONE")
        .replace("USASCII", "UTF-8");
    assert_eq!(name(utf8.as_bytes()), Ok(Some("Bäckerei €".to_owned())));
    let silent = text.replace("CHARSET:1252\n", "");
    assert_eq!(name(silent.as_bytes()), Ok(Some("Bäckerei €".to_owned())));
    // A byte that is not UTF-8 in a file that says it is.
    assert!(matches!(
        name(&bytes_with("UTF-8")),
        Err(ImportError::Malformed { .. })
    ));
    // A character set without a table is refused, not read as another one.
    let other = text.replace("CHARSET:1252", "CHARSET:1250");
    assert_eq!(name(other.as_bytes()), Err(malformed(6)));
    let xml = "<?xml version=\"1.0\" encoding=\"ISO-8859-2\"?>\n<OFX></OFX>";
    assert_eq!(read(xml), Err(malformed(1)));
}

/// A file with Windows-1252 text that declares `encoding` as its encoding.
fn bytes_with(encoding: &str) -> Vec<u8> {
    let mut bytes = file(&transaction("-1.00", "<NAME>B\u{0}\n"))
        .replace("USASCII", encoding)
        .into_bytes();
    let at = bytes
        .iter()
        .position(|&byte| byte == 0)
        .expect("the placeholder");
    bytes[at] = 0xE4;
    bytes
}

#[test]
fn a_byte_order_mark_is_skipped() {
    let text = format!("\u{feff}{}", file(""));
    assert!(read(&text).is_ok());
}

#[test]
fn a_file_that_is_no_statement_is_malformed() {
    assert_eq!(read(""), Err(malformed(1)));
    assert_eq!(read("Date;Amount\n2026-10-02;1,00\n"), Err(malformed(1)));
    assert_eq!(read("<OFX></OFX>"), Err(malformed(1)));
    // Text before the header or the root.
    assert_eq!(read(&format!("Hello\n{}", file(""))), Err(malformed(1)));
    // The file is cut off, or ends in the wrong place.
    let full = file(&transaction("-1.00", ""));
    assert!(matches!(
        read(&full[..full.len() - 40]),
        Err(ImportError::Malformed { .. })
    ));
    assert!(matches!(
        read(&full.replace("</BANKTRANLIST>", "</LEDGERBAL>")),
        Err(ImportError::Malformed { .. })
    ));
    assert!(matches!(
        read(&format!("{full}junk")),
        Err(ImportError::Malformed { .. })
    ));
    assert!(matches!(
        read(&format!("{full}<OFX></OFX>")),
        Err(ImportError::Malformed { .. })
    ));
}

#[test]
fn markup_with_a_bang_is_refused() {
    for markup in [
        "<!DOCTYPE OFX [<!ENTITY x \"y\">]>",
        "<!-- comment -->",
        "<![CDATA[x]]>",
    ] {
        let text = file("").replace("<BANKTRANLIST>", &format!("{markup}<BANKTRANLIST>"));
        assert!(
            matches!(read(&text), Err(ImportError::Malformed { .. })),
            "{markup}"
        );
    }
    let text = format!("<?xml version=\"1.0\"?>\n<!DOCTYPE OFX>\n{}", file(""));
    assert!(matches!(read(&text), Err(ImportError::Malformed { .. })));
}

#[test]
fn a_field_twice_or_a_value_in_an_element_of_elements_is_refused() {
    assert!(matches!(
        read(&file(&transaction("-1.00", "<TRNAMT>-2.00\n"))),
        Err(ImportError::Malformed { .. })
    ));
    assert!(matches!(
        read(&file("").replace("<BANKTRANLIST>\n", "<BANKTRANLIST>junk\n")),
        Err(ImportError::Malformed { .. })
    ));
    assert!(matches!(
        read(&file("").replace("<BANKTRANLIST>\n", "<BANKTRANLIST a=\"b\">\n")),
        Err(ImportError::Malformed { .. })
    ));
}

#[test]
fn the_limits_are_kept() {
    let limits = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    let three = transaction("-1.00", "").repeat(3);
    assert_eq!(
        read_with(file(&three).as_bytes(), &limits),
        Err(ImportError::TooManyEntries { max: 2 })
    );

    let limits = Limits {
        max_text_bytes: 30,
        ..Limits::default()
    };
    let long = file(&transaction(
        "-1.00",
        "<MEMO>much too long for the limit of thirty\n",
    ));
    assert_eq!(
        read_with(long.as_bytes(), &limits),
        Err(ImportError::TextTooLong {
            max_bytes: 30,
            line: Some(26)
        })
    );

    let limits = Limits {
        max_depth: 5,
        ..Limits::default()
    };
    assert_eq!(
        read_with(file(&transaction("-1.00", "")).as_bytes(), &limits),
        Err(ImportError::TooDeeplyNested {
            max: 5,
            line: Some(23)
        })
    );
    let deep = file("").replace(
        "</BANKMSGSRSV1>",
        &format!("{}{}</BANKMSGSRSV1>", "<X>".repeat(70), "</X>".repeat(70)),
    );
    assert!(matches!(
        read(&deep),
        Err(ImportError::TooDeeplyNested { max: 64, .. })
    ));
}

#[test]
fn many_ampersands_are_read_in_linear_time() {
    let name = "&".repeat(16 * 1024);
    let entry = only_entry(&transaction("-1.00", &format!("<NAME>{name}\n")));
    assert_eq!(entry.other_party_name.as_deref(), Some(name.as_str()));
}

#[test]
fn a_file_of_unread_elements_costs_no_memory_for_them() {
    let filler = "<X>\n<Y>value\n</X>\n".repeat(200_000);
    let text = file("").replace("</BANKMSGSRSV1>", &format!("{filler}</BANKMSGSRSV1>"));
    assert_eq!(read(&text).expect("read").len(), 1);
}

proptest! {
    /// Every digit of an amount arrives, with its sign.
    #[test]
    fn amounts_are_exact(minor in -i64::MAX / 2..=i64::MAX / 2, comma in any::<bool>()) {
        let magnitude = minor.unsigned_abs();
        let mark = if comma { ',' } else { '.' };
        let sign = if minor < 0 { "-" } else { "" };
        let text = format!("{sign}{}{mark}{:02}", magnitude / 100, magnitude % 100);
        let entry = only_entry(&transaction(&text, ""));
        prop_assert_eq!(entry.amount.minor_units(), minor);
    }

    /// Every valid day arrives as written, whatever time and zone follow.
    #[test]
    fn dates_are_the_written_day(
        year in 1900_i16..=2100,
        month in 1_i8..=12,
        day in 1_i8..=28,
        time in prop_oneof![Just(String::new()), Just("120000".to_owned()), Just("1200".to_owned())],
        zone in prop_oneof![Just(String::new()), Just("[-5:EST]".to_owned())],
    ) {
        let text = format!("{year:04}{month:02}{day:02}{time}{zone}");
        let entry = only_entry(&format!("<STMTTRN>\n<DTPOSTED>{text}\n<TRNAMT>1.00\n</STMTTRN>\n"));
        prop_assert_eq!(entry.booking_date, date(year, month, day));
    }

    /// No input makes the importer panic, whatever it reads.
    #[test]
    fn any_bytes_give_a_result_or_an_error(input in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = read_with(&input, &Limits::default());
        let _ = Ofx.recognize(&input);
    }

    /// Nor does text made of the importer's own punctuation.
    #[test]
    fn text_of_tags_gives_a_result_or_an_error(
        parts in proptest::collection::vec(
            prop_oneof![
                Just("<OFX>".to_owned()),
                Just("</OFX>".to_owned()),
                Just("<BANKMSGSRSV1>".to_owned()),
                Just("<STMTTRNRS>".to_owned()),
                Just("<STMTRS>".to_owned()),
                Just("</STMTRS>".to_owned()),
                Just("<CURDEF>EUR".to_owned()),
                Just("<BANKTRANLIST>".to_owned()),
                Just("<STMTTRN>".to_owned()),
                Just("</STMTTRN>".to_owned()),
                Just("<TRNAMT>-1.00".to_owned()),
                Just("<DTPOSTED>20261002".to_owned()),
                Just("<LEDGERBAL>".to_owned()),
                Just("</LEDGERBAL>".to_owned()),
                Just("<NAME/>".to_owned()),
                Just("&amp;&#x41;&".to_owned()),
                Just("<!".to_owned()),
                Just("<?".to_owned()),
                Just("\n".to_owned()),
                Just("Grüße".to_owned()),
                "[ -~äöü]{0,12}",
            ],
            0..40,
        )
    ) {
        let _ = read(&parts.concat());
    }

    /// Nor does a file that is cut off or has a byte changed.
    #[test]
    fn a_damaged_file_gives_a_result_or_an_error(cut in 0_usize..700, at in 0_usize..700, byte in any::<u8>()) {
        let full = file(&transaction("-12.50", "<FITID>7\n<NAME>Name\n<MEMO>Memo\n")).into_bytes();
        let mut damaged = full[..cut.min(full.len())].to_vec();
        if let Some(slot) = damaged.get_mut(at) {
            *slot = byte;
        }
        let _ = read_with(&damaged, &Limits::default());
    }
}
