// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Text from a book, written to the terminal.

/// `text` with control characters replaced by `?`. Names and payees come from
/// the book, which may contain anything; written as they are, they could move
/// the cursor or change the terminal's title (AGENTS.md §8).
pub(crate) fn plain(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Writes rows as a table: every column as wide as its widest entry, columns
/// separated by two spaces, no trailing spaces. The first row is the header.
pub(crate) fn table(rows: &[Vec<String>], right_aligned: &[usize]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut text = String::new();
    for row in rows {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate() {
            if column > 0 {
                line.push_str("  ");
            }
            let padding = widths[column].saturating_sub(cell.chars().count());
            if right_aligned.contains(&column) {
                line.push_str(&" ".repeat(padding));
                line.push_str(cell);
            } else {
                line.push_str(cell);
                line.push_str(&" ".repeat(padding));
            }
        }
        text.push_str(line.trim_end());
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_are_replaced() {
        assert_eq!(plain("a\u{1b}[31mb\n"), "a?[31mb?");
        assert_eq!(plain("Café"), "Café");
    }

    #[test]
    fn tables_align_columns() {
        let rows = vec![
            vec!["Name".to_owned(), "Amount".to_owned()],
            vec!["Cash".to_owned(), "5.00".to_owned()],
            vec!["Savings account".to_owned(), "-12.50".to_owned()],
        ];
        assert_eq!(
            table(&rows, &[1]),
            "Name             Amount\nCash               5.00\nSavings account  -12.50\n"
        );
    }
}
