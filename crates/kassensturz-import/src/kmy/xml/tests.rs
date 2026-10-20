// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The elements of `xml` as `path/NAME key=value …`, one per element.
fn elements(xml: &str, limits: &Limits) -> Result<Vec<String>, ImportError> {
    let mut seen = Vec::new();
    read_elements(xml.as_bytes(), limits, |path, element| {
        let mut line = path
            .iter()
            .map(|name| format!("{name}/"))
            .collect::<String>();
        line.push_str(&element.name);
        for (key, value) in &element.attributes {
            line.push_str(&format!(" {key}={value:?}"));
        }
        line.push_str(&format!(" @{}", element.line));
        seen.push(line);
        Ok(())
    })?;
    Ok(seen)
}

fn malformed(line: u64) -> Result<Vec<String>, ImportError> {
    Err(ImportError::Malformed { line: Some(line) })
}

const HEAD: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE KMYMONEY-FILE>\n";

#[test]
fn elements_come_with_where_they_sit_and_their_attributes_unescaped() {
    let xml = format!(
        "{HEAD}<ROOT>\n <LIST count=\"2\">\n  <ITEM name=\"A &amp; B\" memo=\"one&#xa;two\"/>\n  \
         <ITEM name=\"&lt;C&gt;\"></ITEM>\n </LIST>\n</ROOT>\n"
    );
    assert_eq!(
        elements(&xml, &Limits::default()),
        Ok(vec![
            "ROOT @3".to_owned(),
            "ROOT/LIST count=\"2\" @4".to_owned(),
            "ROOT/LIST/ITEM name=\"A & B\" memo=\"one\\ntwo\" @5".to_owned(),
            "ROOT/LIST/ITEM name=\"<C>\" @6".to_owned(),
        ])
    );
    // Without a declaration and document type, after a byte order mark.
    assert_eq!(
        elements("\u{feff}<ROOT/>", &Limits::default()),
        Ok(vec!["ROOT @1".to_owned()])
    );
}

#[test]
fn nothing_is_expanded_or_fetched() {
    // Definitions in the document type could declare entities.
    let defined = "<?xml version=\"1.0\"?>\n<!DOCTYPE KMYMONEY-FILE [<!ENTITY x \"y\">]>\n<ROOT/>";
    assert_eq!(elements(defined, &Limits::default()), malformed(2));
    let external = "<!DOCTYPE KMYMONEY-FILE SYSTEM \"http://example.org/x.dtd\">\n<ROOT/>";
    assert_eq!(elements(external, &Limits::default()), malformed(1));
    let other = "<!DOCTYPE html>\n<ROOT/>";
    assert_eq!(elements(other, &Limits::default()), malformed(1));
    // An entity the file would have to define.
    let undefined = format!("{HEAD}<ROOT name=\"&x;\"/>");
    assert_eq!(elements(&undefined, &Limits::default()), malformed(3));
    let in_text = format!("{HEAD}<ROOT>&x;</ROOT>");
    assert!(matches!(
        elements(&in_text, &Limits::default()),
        Err(ImportError::Malformed { .. })
    ));
}

#[test]
fn only_elements_and_their_attributes_are_data() {
    let text = format!("{HEAD}<ROOT>\n  text\n</ROOT>");
    assert!(matches!(
        elements(&text, &Limits::default()),
        Err(ImportError::Malformed { .. })
    ));
    let cdata = format!("{HEAD}<ROOT><![CDATA[x]]></ROOT>");
    assert!(matches!(
        elements(&cdata, &Limits::default()),
        Err(ImportError::Malformed { .. })
    ));
    // Comments and processing instructions carry nothing and are passed.
    let comment = format!("{HEAD}<!-- saved -->\n<ROOT><?note x?></ROOT>");
    assert_eq!(
        elements(&comment, &Limits::default()),
        Ok(vec!["ROOT @4".to_owned()])
    );
}

#[test]
fn a_file_is_one_whole_element() {
    let second = format!("{HEAD}<ROOT/>\n<ROOT/>");
    assert_eq!(elements(&second, &Limits::default()), malformed(4));
    let after = format!("{HEAD}<ROOT></ROOT>\n<!DOCTYPE KMYMONEY-FILE>");
    assert_eq!(elements(&after, &Limits::default()), malformed(4));
    let unclosed = format!("{HEAD}<ROOT>\n<LIST>\n</LIST>\n");
    assert_eq!(elements(&unclosed, &Limits::default()), malformed(6));
    let crossed = format!("{HEAD}<ROOT><LIST></ROOT></LIST>");
    assert!(matches!(
        elements(&crossed, &Limits::default()),
        Err(ImportError::Malformed { .. })
    ));
    assert!(matches!(
        elements("", &Limits::default()),
        Err(ImportError::Malformed { .. })
    ));
}

#[test]
fn the_file_is_utf_8_xml_1_0() {
    let latin = b"<ROOT name=\"Caf\xe9\"/>";
    assert_eq!(
        read_elements(latin, &Limits::default(), |_, _| Ok(())),
        Err(ImportError::Malformed { line: Some(1) })
    );
    let declared = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<ROOT/>";
    assert_eq!(elements(declared, &Limits::default()), malformed(1));
    let version = "<?xml version=\"1.1\"?>\n<ROOT/>";
    assert_eq!(elements(version, &Limits::default()), malformed(1));
}

#[test]
fn the_limits_are_kept_while_reading() {
    let deep = Limits {
        max_depth: 2,
        ..Limits::default()
    };
    assert!(elements("<A><B/></A>", &deep).is_ok());
    assert_eq!(
        elements("<A>\n<B>\n<C/></B></A>", &deep),
        Err(ImportError::TooDeeplyNested {
            max: 2,
            line: Some(3),
        })
    );
    let short = Limits {
        max_text_bytes: 3,
        ..Limits::default()
    };
    assert!(elements("<A name=\"&amp;bc\"/>", &short).is_ok());
    assert_eq!(
        elements("<A>\n<B name=\"abcd\"/></A>", &short),
        Err(ImportError::TextTooLong {
            max_bytes: 3,
            line: Some(2),
        })
    );
}

#[test]
fn a_refusal_of_the_reader_stops_reading() {
    let mut seen = 0;
    let refused = read_elements(b"<A><B/><C/></A>", &Limits::default(), |_, element| {
        seen += 1;
        if element.name == "B" {
            Err(ImportError::Malformed {
                line: Some(element.line),
            })
        } else {
            Ok(())
        }
    });
    assert_eq!(refused, Err(ImportError::Malformed { line: Some(1) }));
    assert_eq!(seen, 2);
}
