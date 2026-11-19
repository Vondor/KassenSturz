# Answer sheet: Suburban Family (English, the Summerfield Family)

This sheet is written by the test `example_books` (`crates/kassensturz-pipeline/tests/example_books.rs`) from the real import and report code, not by hand. The book and the statements are made by `tools/example-books/generate.py`; nothing in them belongs to a real person.

Reference month: 2026-11 (last month is October 2026). The book goes back about two years. If you make it in a later month, KassenSturz moves every date by whole months so that the reference month is the running month; the figures below are then those of the month before the running one. KassenSturz leaves out bookings after today. The statements match the book only in the reference month itself.

## Card 4: Overview

- Spent on “Food” in October 2026: **1,352.35 EUR**
- Money in during October 2026: 9,586.40 EUR; money out: 5,833.62 EUR; difference: 3,752.78 EUR
- Did more come in than go out? **Yes**

## Card 5: Read a bank statement (account “Checking account”)

Both files are in `crates/kassensturz-import/tests/books/examples/statements/` and hold the same 9 lines; exactly one of them is already in the book.

| File | Lines | new | already in the book (duplicate) | looks the same (you decide) | closing balance from the bank | balance after reading |
|---|---|---|---|---|---|---|
| `suburban-family-en.camt053.xml` | 9 | 8 | 1 | 0 | 27,037.90 EUR | 27,037.90 EUR |
| `suburban-family-en.csv` | 9 | 8 | 0 | 1 | (a CSV file has none) | – |

A CAMT file carries the bank's number for every line, so KassenSturz safely finds the line that is already there as a duplicate. A CSV file has no such number; the line that is already there has the same amount and almost the same date as a booking in the book, so it is only shown as “looks the same” and you decide whether to read it anyway. The other 8 lines are new. Reading the CAMT file first and the CSV file after it, KassenSturz finds every line already in the book.

The CSV settings (delimiter “,”, decimal point, year-month-day, columns Booking date, Value date, Payee, Payee IBAN, Reference, Amount, Currency) are in `suburban-family-en.csv.settings`.
