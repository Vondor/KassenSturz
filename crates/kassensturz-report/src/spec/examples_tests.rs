// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Every ready example (US4.11, D-720) is read in full: no part that is not
//! supported, something to draw, and the data name it says it uses. That
//! each also draws from a book is tested in `kassensturz-ui-bridge`.

use super::*;

#[test]
fn every_example_is_read_whole_and_names_its_data() {
    assert_eq!(SPEC_EXAMPLES.len(), 4);
    for example in SPEC_EXAMPLES {
        let read = read(example.text).unwrap_or_else(|e| panic!("{}: {e:?}", example.key));
        assert!(
            read.unsupported().is_empty(),
            "{}: {:?}",
            example.key,
            read.unsupported()
        );
        let (chart, _) = read.into_parts();
        let chart = chart.unwrap_or_else(|| panic!("{} has nothing to draw", example.key));
        assert_eq!(chart.dataset().id(), example.dataset, "{}", example.key);
        assert!(example.text.len() <= MAX_SPEC_BYTES);
    }
    let mut keys: Vec<&str> = SPEC_EXAMPLES.iter().map(|e| e.key).collect();
    keys.dedup();
    assert_eq!(keys.len(), SPEC_EXAMPLES.len());
    // The first is the bars-plus-lines chart the help promises.
    assert!(
        SPEC_EXAMPLES[0].text.contains("\"bar\"") && SPEC_EXAMPLES[0].text.contains("\"line\"")
    );
}
