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
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&plain_line(line));
    }
    out
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
