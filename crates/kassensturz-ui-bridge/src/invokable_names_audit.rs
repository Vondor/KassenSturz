// SPDX-FileCopyrightText: 2026 KassenSturz contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A test that reads the bridge's source for the names QML calls: cxx-qt
//! does not turn `snake_case` into `camelCase` for a `#[qinvokable]`, so one
//! without `#[cxx_name = "camelCase"]` is called `some_name` in QML, and the
//! page that calls `someName` fails with "is not a function" (B-351: the
//! period check of a formula never ran, and the stand-in used by the QML
//! smoke tests hid it). QML isn't run against the real objects by the unit
//! tests, so this keeps a new invokable from going without.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    fn camel(name: &str) -> String {
        let mut out = String::new();
        let mut up = false;
        for c in name.chars() {
            if c == '_' {
                up = true;
            } else if up {
                out.extend(c.to_uppercase());
                up = false;
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn an_invokable_with_an_underscore_has_its_camel_case_name() {
        let mut wrong = Vec::new();
        let mut seen = 0;
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in fs::read_dir(src).expect("the source folder") {
            let path = entry.expect("a file").path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = fs::read_to_string(&path).expect("source text");
            let lines: Vec<&str> = text.lines().collect();
            for (at, line) in lines.iter().enumerate() {
                if line.trim() != "#[qinvokable]" {
                    continue;
                }
                seen += 1;
                let mut cxx_name = None;
                let mut next = at + 1;
                while !lines[next].trim_start().starts_with("fn ") {
                    if let Some(rest) = lines[next].trim().strip_prefix("#[cxx_name = \"") {
                        cxx_name = rest.strip_suffix("\"]").map(str::to_owned);
                    }
                    next += 1;
                }
                let name = lines[next]
                    .trim_start()
                    .trim_start_matches("fn ")
                    .split(['(', '<'])
                    .next()
                    .expect("a name");
                if name.contains('_') && cxx_name.as_deref() != Some(camel(name).as_str()) {
                    wrong.push(format!("{}: {name}", path.display()));
                }
            }
        }
        assert!(seen > 50, "the audit found only {seen} invokables");
        assert!(
            wrong.is_empty(),
            "no #[cxx_name] with the camelCase name: {wrong:?}"
        );
    }
}
