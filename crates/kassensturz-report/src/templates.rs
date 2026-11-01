// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in starter dashboards, by the word a dashboard made from one
//! keeps in its `template` key (D-439, D-440). M6d.1 starts the set with
//! `overview` (the built-in Overview, D-314) and `empty`; M6d.2 adds the
//! others, each with its word here and its document in [`document`].
//!
//! A template holds no English text a user would see: titles stay empty so
//! each component shows its own translated default, and the dashboard's name
//! is given by the screen that makes it (`docs/REPORT_DESIGNER_M6D.md`,
//! section 2).

use kassensturz_core::ViewKind;

use crate::{Document, OVERVIEW_NAME, ReportError};

/// The template of the built-in Overview.
pub const OVERVIEW: &str = "overview";
/// The template with no components.
pub const EMPTY: &str = "empty";

/// Every word the `template` key may hold, in the order a list shows them.
pub const WORDS: [&str; 2] = [EMPTY, OVERVIEW];

/// The document of the template `word`, carrying the word in its `template`
/// key, named after the word until the screen gives it the user's name. A
/// word that is not one of [`WORDS`] is refused (`report.document-invalid`,
/// `bad-choice`, path `template`).
pub fn document(word: &str) -> Result<Document, ReportError> {
    let mut document = match word {
        OVERVIEW => Document::default_overview()?,
        EMPTY => Document {
            kind: ViewKind::Dashboard,
            name: OVERVIEW_NAME.to_owned(),
            period: None,
            accounts: Vec::new(),
            categories: Vec::new(),
            entries: Vec::new(),
            template: None,
            default: false,
            origin: None,
        },
        _ => {
            return Err(ReportError::DocumentInvalid {
                reason: "bad-choice",
                path: "template".to_owned(),
            });
        }
    };
    document.template = Some(word.to_owned());
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Item;

    #[test]
    fn every_template_reads_back_as_a_dashboard_with_its_word() {
        for word in WORDS {
            let document = document(word).unwrap();
            assert_eq!(document.kind, ViewKind::Dashboard);
            assert_eq!(document.template.as_deref(), Some(word));
            assert!(!document.default);
            let again = Document::read(&document.to_json()).unwrap();
            assert_eq!(again, document);
        }
    }

    #[test]
    fn the_overview_template_is_the_built_in_overview() {
        let template = document(OVERVIEW).unwrap();
        let built_in = Document::default_overview().unwrap();
        assert_eq!(template.entries, built_in.entries);
        assert_eq!(template.filters(), built_in.filters());
        assert!(
            template
                .entries
                .iter()
                .all(|entry| matches!(entry.item, Item::Component(_)))
        );
    }

    #[test]
    fn the_empty_template_has_no_components() {
        assert!(document(EMPTY).unwrap().entries.is_empty());
    }

    #[test]
    fn a_word_that_is_not_a_template_is_refused() {
        for word in ["", "Overview", "spending", "nonsense"] {
            assert!(matches!(
                document(word),
                Err(ReportError::DocumentInvalid {
                    reason: "bad-choice",
                    ..
                })
            ));
        }
    }
}
