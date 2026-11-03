# Tester kit

Everything a person needs to test KassenSturz without help from the code. German is the language of the testers (D-553); the English files are the team's copies. Testers send their reports privately to the maintainer (D-552).

| For | German | English |
|---|---|---|
| Testers: what the test is, safety, consent, install overview | [TESTER_GUIDE.de.md](TESTER_GUIDE.de.md) | [TESTER_GUIDE.en.md](TESTER_GUIDE.en.md) |
| Testers: install on Windows | [INSTALL_WINDOWS.de.md](INSTALL_WINDOWS.de.md) | [INSTALL_WINDOWS.en.md](INSTALL_WINDOWS.en.md) |
| Testers: install on Ubuntu | [INSTALL_UBUNTU.de.md](INSTALL_UBUNTU.de.md) | [INSTALL_UBUNTU.en.md](INSTALL_UBUNTU.en.md) |
| Testers: questionnaire and task cards | [QUESTIONNAIRE_1.de.md](QUESTIONNAIRE_1.de.md) | [QUESTIONNAIRE_1.en.md](QUESTIONNAIRE_1.en.md) |
| Testers: report a problem | [REPORT_PAGE.de.md](REPORT_PAGE.de.md) | [REPORT_PAGE.en.md](REPORT_PAGE.en.md) |
| Facilitator: script, consent step, success criteria | [FACILITATOR_SCRIPT.de.md](FACILITATOR_SCRIPT.de.md) | [FACILITATOR_SCRIPT.en.md](FACILITATOR_SCRIPT.en.md) |
| Everyone: how to use KassenSturz | [USER_GUIDE.de.md](../user/USER_GUIDE.de.md) | not yet written |

## Answer sheets and bank files

The example books, their matching bank statements and the facilitator's answer sheets come from step M7a.8. The generator is `tools/example-books/generate.py`; the files are under `crates/kassensturz-import/tests/books/examples/` (the books are Suburban Family, Flower Utopia, Space Colony and USSR Government Agent; in German "Vorstadtfamilie", "Blumen-Utopia", "Weltraumkolonie" and "UdSSR-Agent").

- **Bank files for card 5** (the facilitator puts the matching ones on the tester's computer): `statements/<book>.csv` with its column settings `statements/<book>.csv.settings`, and `statements/<book>.camt053.xml`. `<book>` is `suburban-family`, `flower-utopia`, `space-colony` or `udssr-agent`.
- **Answer sheets for cards 4 and 5** (German, facilitator only, never shown to the tester): `answers/<book>.md`. They are written by a test from the real import and report code. The CSV and the CAMT file give different counts on purpose (the CSV has no bank number, so the line already in the book is shown as "looks the same", not as a duplicate).

## Still to be filled in

These need things that do not exist yet; each is marked "Bildschirmfoto folgt" or "wird nachgetragen" in the sheets.

- **Screenshots** from a clean machine for every click. Windows: the first VM run (M7a.7, 2026-11-03) is in `screenshots/windows/`, numbered in the order of the sheet and the task cards. Its installer pages show Craft's earlier installer (English, with an administrator prompt), so they go into the sheet after the second run with KassenSturz's own installer. Ubuntu: from a clean Ubuntu with the first pre-release installed, in `screenshots/ubuntu/`.

## Rules for this folder

Only synthetic data. No real tester report, recording or note is ever stored here, in an issue or in the project files (D-552).
