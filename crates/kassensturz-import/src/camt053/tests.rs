// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use proptest::prelude::*;

use super::*;
use crate::import;

const NAMESPACE_02: &str = "urn:iso:std:iso:20022:tech:xsd:camt.053.001.02";

/// A document with one statement in euros around `body`.
fn document(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <Document xmlns=\"{NAMESPACE_02}\"><BkToCstmrStmt><Stmt>\
         <Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy></Acct>\
         {body}</Stmt></BkToCstmrStmt></Document>"
    )
}

/// A booked entry of `amount` with `indicator`.
fn entry(amount: &str, indicator: &str, extra: &str) -> String {
    format!(
        "<Ntry><Amt Ccy=\"EUR\">{amount}</Amt><CdtDbtInd>{indicator}</CdtDbtInd>\
         <Sts>BOOK</Sts><BookgDt><Dt>2026-10-01</Dt></BookgDt>{extra}</Ntry>"
    )
}

fn read(xml: &str) -> Result<Vec<Statement>, ImportError> {
    read_with(xml.as_bytes(), &Limits::default())
}

fn read_with(input: &[u8], limits: &Limits) -> Result<Vec<Statement>, ImportError> {
    match import(&Camt053, input, limits)? {
        Imported::Statements(statements) => Ok(statements),
        other => panic!("a statement importer read {other:?}"),
    }
}

fn only_entry(xml: &str) -> Entry {
    let mut statements = read(xml).expect("the document is read");
    assert_eq!(statements.len(), 1);
    let mut entries = statements.remove(0).entries;
    assert_eq!(entries.len(), 1);
    entries.remove(0)
}

#[test]
fn the_namespace_of_any_version_is_recognized() {
    let recognize = |text: &str| Camt053.recognize(text.as_bytes());
    for version in ["01", "02", "08", "12"] {
        let namespace = format!("urn:iso:std:iso:20022:tech:xsd:camt.053.001.{version}");
        assert_eq!(
            recognize(&format!(
                "<?xml version=\"1.0\"?><Document xmlns=\"{namespace}\">"
            )),
            Recognition::Yes
        );
    }
    assert_eq!(
        recognize("<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.052.001.02\">"),
        Recognition::No
    );
    assert_eq!(recognize("<Document>"), Recognition::No);
    assert_eq!(recognize(""), Recognition::No);
    assert_eq!(recognize("Date;Amount"), Recognition::No);
}

#[test]
fn the_importer_describes_itself() {
    let info = Camt053.info();
    assert_eq!(info.id, "camt053");
    assert_eq!(info.kind, ImportKind::Statements);
    assert!(info.extensions.contains(&"xml"));
}

#[test]
fn a_byte_order_mark_is_skipped() {
    let mut input = vec![0xEF, 0xBB, 0xBF];
    input.extend(document(&entry("1.00", "CRDT", "")).as_bytes());
    assert_eq!(
        read_with(&input, &Limits::default()).map(|s| s.len()),
        Ok(1)
    );
}

#[test]
fn invalid_utf8_is_malformed_on_its_line() {
    let mut input = document("").into_bytes();
    input.splice(0..0, *b"\n\n");
    input.push(0xFF);
    assert_eq!(
        read_with(&input, &Limits::default()),
        Err(ImportError::Malformed { line: Some(4) })
    );
}

#[test]
fn text_is_read_as_the_file_delivered_it() {
    let extra = "<NtryDtls><TxDtls><RmtInf>\
        <Ustrd>Fish &amp; chips &#65;&#x42; &lt;b&gt; &quot;q&quot; &apos;a&apos;</Ustrd>\
        <Ustrd><![CDATA[<raw> & text]]></Ustrd>\
        <Ustrd>  padded  </Ustrd>\
        <Ustrd>   </Ustrd>\
        <Ustrd/>\
        <Ustrd>line\r\nbreak</Ustrd>\
        </RmtInf></TxDtls></NtryDtls>";
    let entry = only_entry(&document(&entry("1.00", "CRDT", extra)));
    assert_eq!(
        entry.remittance_information.as_deref(),
        Some("Fish & chips AB <b> \"q\" 'a'\n<raw> & text\n  padded  \nline\nbreak")
    );
}

#[test]
fn text_in_elements_the_importer_does_not_read_is_ignored() {
    let extra = "<AddtlNtryInf>Booking text</AddtlNtryInf><NtryDtls><TxDtls>\
        <RltdPties><UltmtDbtr><Nm>Ultimate</Nm></UltmtDbtr></RltdPties>\
        <AddtlTxInf>More</AddtlTxInf></TxDtls></NtryDtls>";
    let entry = only_entry(&document(&entry("1.00", "CRDT", extra)));
    assert_eq!(entry.other_party_name, None);
    assert_eq!(entry.remittance_information, None);
}

#[test]
fn the_direction_of_a_reversal_is_the_one_in_the_file() {
    let parties = "<NtryDtls><TxDtls><RltdPties>\
        <Dbtr><Nm>Payer</Nm></Dbtr><Cdtr><Nm>Payee</Nm></Cdtr>\
        </RltdPties></TxDtls></NtryDtls>";
    let name = |indicator: &str, reversal: &str| {
        let extra = format!("<RvslInd>{reversal}</RvslInd>{parties}");
        let entry = only_entry(&document(&entry("1.00", indicator, &extra)));
        (entry.amount.minor_units(), entry.other_party_name)
    };
    assert_eq!(name("CRDT", "false"), (100, Some("Payer".to_owned())));
    assert_eq!(name("DBIT", "false"), (-100, Some("Payee".to_owned())));
    assert_eq!(name("CRDT", "true"), (100, Some("Payee".to_owned())));
    assert_eq!(name("DBIT", "true"), (-100, Some("Payer".to_owned())));
}

#[test]
fn a_status_other_than_booked_is_pending() {
    let status = |status: &str| {
        let xml = document(&format!(
            "<Ntry><Amt Ccy=\"EUR\">1.00</Amt><CdtDbtInd>CRDT</CdtDbtInd>{status}\
             <BookgDt><Dt>2026-10-01</Dt></BookgDt></Ntry>"
        ));
        only_entry(&xml).status
    };
    assert_eq!(status("<Sts>BOOK</Sts>"), EntryStatus::Booked);
    assert_eq!(status("<Sts><Cd>BOOK</Cd></Sts>"), EntryStatus::Booked);
    assert_eq!(status("<Sts>PDNG</Sts>"), EntryStatus::Pending);
    assert_eq!(status("<Sts>INFO</Sts>"), EntryStatus::Pending);
    assert_eq!(status("<Sts>book</Sts>"), EntryStatus::Pending);
    assert_eq!(
        status("<Sts><Prtry>BOOK</Prtry></Sts>"),
        EntryStatus::Pending
    );
}

#[test]
fn only_the_banks_own_reference_is_the_bank_id() {
    let id = |extra: &str| only_entry(&document(&entry("1.00", "CRDT", extra))).bank_id;
    // `NtryRef` is a running number at some banks, so it never counts.
    assert_eq!(id("<NtryRef>1</NtryRef>"), None);
    assert_eq!(
        id("<NtryRef>1</NtryRef><AcctSvcrRef>REF-7</AcctSvcrRef>").as_deref(),
        Some("REF-7")
    );
    assert_eq!(id("<AcctSvcrRef>nonref</AcctSvcrRef>"), None);
    assert_eq!(id("<AcctSvcrRef>NOTPROVIDED</AcctSvcrRef>"), None);
    assert_eq!(id("<AcctSvcrRef>  </AcctSvcrRef>"), None);
    let only_in_the_payment = "<NtryDtls><TxDtls><Refs><AcctSvcrRef>TX-1</AcctSvcrRef></Refs>\
        </TxDtls></NtryDtls>";
    assert_eq!(id(only_in_the_payment).as_deref(), Some("TX-1"));
    // With several payments, none of their references stands for the entry.
    let two = "<NtryDtls><TxDtls><Refs><AcctSvcrRef>TX-1</AcctSvcrRef></Refs></TxDtls>\
        <TxDtls><Refs><AcctSvcrRef>TX-2</AcctSvcrRef></Refs></TxDtls></NtryDtls>";
    assert_eq!(id(two), None);
}

#[test]
fn dates_are_calendar_days_whatever_follows() {
    let day = |value: &str| {
        let xml = document(&format!(
            "<Ntry><Amt Ccy=\"EUR\">1.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><Sts>BOOK</Sts>\
             <BookgDt><Dt>{value}</Dt></BookgDt></Ntry>"
        ));
        read(&xml).map(|mut s| s.remove(0).entries.remove(0).booking_date.to_string())
    };
    assert_eq!(day("2026-10-01").as_deref(), Ok("2026-10-01"));
    assert_eq!(day("2026-10-01+02:00").as_deref(), Ok("2026-10-01"));
    assert_eq!(day("2026-10-01Z").as_deref(), Ok("2026-10-01"));
    // Not converted into another time zone: the day the bank wrote counts.
    assert_eq!(
        day("2026-10-01T23:59:59-05:00").as_deref(),
        Ok("2026-10-01")
    );
    for bad in [
        "",
        "2026-10",
        "20261001",
        "2026-02-30",
        "2026-10-01x",
        "01.10.2026",
        "tomorrow",
    ] {
        assert!(
            matches!(day(bad), Err(ImportError::Malformed { .. })),
            "{bad:?} is refused"
        );
    }
}

#[test]
fn a_document_type_is_refused_before_anything_is_expanded() {
    let xml = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE Document [<!ENTITY a \"aaaa\"><!ENTITY b \"&a;&a;&a;&a;\">]>\
         <Document xmlns=\"{NAMESPACE_02}\"><BkToCstmrStmt><Stmt/></BkToCstmrStmt></Document>"
    );
    assert_eq!(read(&xml), Err(ImportError::Malformed { line: Some(1) }));
}

#[test]
fn only_the_first_element_decides_the_namespace() {
    let wrong = "<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.054.001.02\">\
        <BkToCstmrStmt><Stmt/></BkToCstmrStmt></Document>";
    assert!(matches!(read(wrong), Err(ImportError::Malformed { .. })));
    let twice = format!("{}{}", document(""), document(""));
    assert!(matches!(read(&twice), Err(ImportError::Malformed { .. })));
    // Version digits only.
    let odd = document("").replace(".02", ".x2");
    assert!(matches!(read(&odd), Err(ImportError::Malformed { .. })));
}

#[test]
fn the_limits_are_kept_while_reading() {
    let two = document(&format!(
        "{}{}",
        entry("1.00", "CRDT", ""),
        entry("2.00", "CRDT", "")
    ));
    let limits = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    assert_eq!(
        read_with(two.as_bytes(), &limits),
        Err(ImportError::TooManyEntries { max: 1 })
    );
    let limits = Limits {
        max_depth: 4,
        ..Limits::default()
    };
    assert!(matches!(
        read_with(two.as_bytes(), &limits),
        Err(ImportError::TooDeeplyNested {
            max: 4,
            line: Some(_)
        })
    ));
    let named = document(&entry(
        "1.00",
        "CRDT",
        "<NtryDtls><TxDtls><RltdPties><Cdtr><Nm>Twelve chars</Nm></Cdtr></RltdPties></TxDtls></NtryDtls>",
    ));
    let limits = Limits {
        max_text_bytes: 11,
        ..Limits::default()
    };
    assert!(matches!(
        read_with(named.as_bytes(), &limits),
        Err(ImportError::TextTooLong {
            max_bytes: 11,
            line: Some(_)
        })
    ));
}

#[test]
fn balances_of_other_kinds_are_not_read() {
    let balance = |kind: &str, currency: &str| {
        format!(
            "<Bal><Tp><CdOrPrtry><Cd>{kind}</Cd></CdOrPrtry></Tp><Amt Ccy=\"{currency}\">5.00</Amt>\
             <CdtDbtInd>CRDT</CdtDbtInd><Dt><Dt>2026-10-31</Dt></Dt></Bal>"
        )
    };
    // An available balance in another currency doesn't matter.
    let xml = document(&format!(
        "{}{}",
        balance("CLAV", "USD"),
        balance("ITBD", "USD")
    ));
    let statements = read(&xml).expect("read");
    assert_eq!(statements[0].opening_balance, None);
    assert_eq!(statements[0].closing_balance, None);
    // But the closing balance has to be in the statement's currency.
    let xml = document(&balance("CLBD", "USD"));
    assert!(matches!(
        read(&xml),
        Err(ImportError::CurrencyMismatch { .. })
    ));
}

#[test]
fn an_opening_balance_wins_over_the_previous_closing_one() {
    let balance = |kind: &str, amount: &str| {
        format!(
            "<Bal><Tp><CdOrPrtry><Cd>{kind}</Cd></CdOrPrtry></Tp><Amt Ccy=\"EUR\">{amount}</Amt>\
             <CdtDbtInd>CRDT</CdtDbtInd><Dt><Dt>2026-10-01</Dt></Dt></Bal>"
        )
    };
    let xml = document(&format!(
        "{}{}",
        balance("PRCD", "1.00"),
        balance("OPBD", "2.00")
    ));
    let opening = read(&xml).expect("read")[0].opening_balance;
    assert_eq!(opening.map(|b| b.amount.minor_units()), Some(200));
}

proptest! {
    /// Every digit of an amount arrives, with the sign from the indicator.
    #[test]
    fn amounts_are_exact(minor in 0_i64..=i64::MAX / 2, debit in any::<bool>()) {
        let text = format!("{}.{:02}", minor / 100, minor % 100);
        let indicator = if debit { "DBIT" } else { "CRDT" };
        let entry = only_entry(&document(&entry(&text, indicator, "")));
        let expected = if debit { -minor } else { minor };
        prop_assert_eq!(entry.amount.minor_units(), expected);
    }

    /// No input makes the importer panic, whatever it reads.
    #[test]
    fn any_bytes_give_a_result_or_an_error(input in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = read_with(&input, &Limits::default());
        let _ = Camt053.recognize(&input);
    }

    /// Nor does a document that is cut off or has a byte changed.
    #[test]
    fn a_damaged_document_gives_a_result_or_an_error(cut in 0_usize..1200, at in 0_usize..1200, byte in any::<u8>()) {
        let full = document(&entry(
            "12.34",
            "DBIT",
            "<NtryDtls><TxDtls><RltdPties><Cdtr><Nm>Name</Nm></Cdtr></RltdPties></TxDtls></NtryDtls>",
        ))
        .into_bytes();
        let mut damaged = full[..cut.min(full.len())].to_vec();
        if let Some(slot) = damaged.get_mut(at) {
            *slot = byte;
        }
        let _ = read_with(&damaged, &Limits::default());
    }
}
