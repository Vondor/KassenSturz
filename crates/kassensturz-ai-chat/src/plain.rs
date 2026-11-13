// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown marks out of an answer (M5-B46). The chat shows the model's
//! text as plain text on purpose (nothing it writes becomes a link, an
//! image or markup, T-23), so the marks a model adds out of habit, such as
//! `**9703.92 EUR**`, would show as characters. They are taken out here,
//! before the figures in the text are checked, so the text the user reads is
//! the text that was checked.

/// `text` without bold and italic marks, code ticks and heading marks; a
/// list stays a list with a dash. Everything else is left as written.
pub(crate) fn plain_answer(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut at = 0;
    while at < lines.len() {
        if let Some(end) = table_end(&lines, at) {
            out.extend(plain_table(&lines[at..end]));
            at = end;
        } else {
            out.push(plain_line(lines[at]));
            at += 1;
        }
    }
    out.join("\n")
}

/// The cells of a table row: `| a | b |` is `a` and `b`. `None` for a line
/// that isn't a row (it must start and end with a pipe).
fn cells(line: &str) -> Option<Vec<&str>> {
    let line = line.trim();
    let inner = line.strip_prefix('|')?.strip_suffix('|')?;
    Some(inner.split('|').map(str::trim).collect())
}

/// Whether `line` is the row under a table's header: only dashes, colons
/// and spaces in each cell, at least one dash.
fn is_separator(line: &str) -> bool {
    cells(line).is_some_and(|row| {
        row.iter()
            .all(|cell| cell.contains('-') && cell.chars().all(|c| matches!(c, '-' | ':' | ' ')))
    })
}

/// Where the pipe table that starts at `start` ends (the index of the first
/// line after it), if one starts there: a header row, a separator row and
/// any rows after them.
fn table_end(lines: &[&str], start: usize) -> Option<usize> {
    cells(lines.get(start)?)?;
    if !is_separator(lines.get(start + 1)?) {
        return None;
    }
    let mut end = start + 2;
    while lines.get(end).is_some_and(|line| cells(line).is_some()) {
        end += 1;
    }
    Some(end)
}

/// A pipe table as aligned lines: the separator row is dropped, the cells
/// lose their marks, and every column is as wide as its widest cell. The
/// chat shows plain text, so this is what is left of a table.
fn plain_table(lines: &[&str]) -> Vec<String> {
    let rows: Vec<Vec<String>> = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 1)
        .filter_map(|(_, line)| cells(line))
        .map(|row| row.iter().map(|cell| plain_line(cell)).collect())
        .collect();
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
    rows.iter()
        .map(|row| {
            let mut line = String::new();
            for (column, width) in widths.iter().enumerate() {
                let cell = row.get(column).map_or("", String::as_str);
                if column > 0 {
                    line.push_str("   ");
                }
                line.push_str(cell);
                line.push_str(&" ".repeat(width - cell.chars().count()));
            }
            line.trim_end().to_owned()
        })
        .collect()
}

fn plain_line(line: &str) -> String {
    let indent = line.len() - line.trim_start().len();
    let (space, rest) = line.split_at(indent);
    // A heading: `## Title` is just `Title`.
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    let rest = if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        rest[hashes..].trim_start()
    } else {
        rest
    };
    // A bullet written with a star becomes a dash.
    let rest = match rest.strip_prefix("* ") {
        Some(item) => format!("- {item}"),
        None => rest.to_owned(),
    };
    let rest = rest.replace("**", "").replace("__", "").replace('`', "");
    format!("{space}{}", without_single_stars(&rest))
}

/// `*word*` becomes `word`. A star that does not open and close around text
/// on this line (a footnote mark, a multiplication) is left alone.
fn without_single_stars(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '*' {
            let opens = chars.get(at + 1).is_some_and(|c| !c.is_whitespace())
                && (at == 0 || !chars[at - 1].is_alphanumeric());
            if opens
                && let Some(end) = (at + 2..chars.len())
                    .find(|&i| chars[i] == '*' && !chars[i - 1].is_whitespace())
            {
                out.extend(&chars[at + 1..end]);
                at = end + 1;
                continue;
            }
        }
        out.push(chars[at]);
        at += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_italic_code_and_headings_lose_their_marks() {
        assert_eq!(
            plain_answer("You have **9703.92 EUR** in `Checking`, *roughly* half."),
            "You have 9703.92 EUR in Checking, roughly half."
        );
        assert_eq!(
            plain_answer("## Summary\n* one\n  * two"),
            "Summary\n- one\n  - two"
        );
        assert_eq!(plain_answer("__Total__: 5"), "Total: 5");
    }

    #[test]
    fn a_pipe_table_becomes_aligned_lines() {
        let table = "Spending:\n\n| Category | Amount |\n|:---|---:|\n| **Food** | 153.51 EUR |\n| Rent | 900.00 EUR |\n\nDone.";
        assert_eq!(
            plain_answer(table),
            "Spending:\n\nCategory   Amount\nFood       153.51 EUR\nRent       900.00 EUR\n\nDone."
        );
        // A line with pipes that is no table stays.
        assert_eq!(plain_answer("a | b | c"), "a | b | c");
        assert_eq!(plain_answer("| only | a row |"), "| only | a row |");
    }

    #[test]
    fn what_is_no_markdown_stays_as_written() {
        for text in [
            "3 * 4 = 12",
            "Note* the rate",
            "snake_case_name and a_b",
            "#hashtag stays",
            "Fee: 5.00 EUR (see 2026-10-03)",
            "Zeile 1\n\nZeile 3",
        ] {
            assert_eq!(plain_answer(text), text, "{text}");
        }
    }
}
