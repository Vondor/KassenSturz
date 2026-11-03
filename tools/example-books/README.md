# The example books

`generate.py` writes three made-up German example books and the bank files for
all four example books (M7a.8, D-561). Everything in them is invented: no real
person, bank, agency or brand, and only example IBANs from the SWIFT registry
(D-055). The amounts are in believable ranges; the public sources used for that
are named at the top of the script.

| Book | File | What it is |
|---|---|---|
| Suburban Family (suggested start) | `suburban-family-v15.sqlite` | A family of five in North Rhine-Westphalia: a nurse at a public hospital (TVöD-P), a teacher at a Gymnasium (civil servant, A 13), both 40, three children, a terraced house with a mortgage and two cars. Girokonto, Sparkonto, Kreditkarte, Bargeld, the default German categories, Umbuchungen (the credit card paid off every month, cash withdrawn, a monthly saving). |
| Flower Utopia | `flower-utopia-v15.sqlite`, `flower-utopia.json` | A small garden cooperative with a shared cash box (Gemeinschaftskasse), Transfers from the cash box to the club account after each market, and a food category. The `.json` is the complete export with five monthly budgets and a saved Overview with charts (a KMyMoney file cannot hold them). |
| UdSSR Government Agent | `udssr-agent-v15.sqlite` | A fictional 1980s agent of the made-up "Hauptverwaltung für Windstille": rubles, dollars, pounds and francs with agency rates, and a monthly Transfer to the savings book. Only current currency codes are used (D-077). The dates are recent. |
| Space Colony | `../space-colony-5y-v15.sqlite` | The older five-year book. It is read, never changed; only its bank files are made here. Its amounts were compared with the few public analogue figures found and left as they are (D-563). |

All files are in `crates/kassensturz-import/tests/books/examples/`:

```
suburban-family-v15.sqlite  flower-utopia-v15.sqlite  udssr-agent-v15.sqlite
flower-utopia.json          reference-month.txt
statements/<book>.csv  <book>.csv.settings  <book>.camt053.xml   (all four books)
answers/<book>.md                                                   (all four books)
```

## Make them again

```sh
tools/example-books/generate.py                           # reference month 2026-11
tools/example-books/generate.py --reference-month 2027-01
tools/example-books/generate.py --check                   # fails if a file differs
KASSENSTURZ_BLESS=1 cargo test -p kassensturz-pipeline --test example_books
```

The books hold the 24 months before the reference month, so "last month" and
the last twelve months are never empty, and some cash and card spending in the
reference month itself; the statements hold the reference month's first days of
the account they are for, which has no other line in that month. When the app
makes a book from an example, it moves every date by whole months so that the
reference month is the running month and leaves out what lies after today
(D-584). The statements match the book only in the reference month, so a test
round in a later month runs the generator with that month. The
generator saves the reference month in `reference-month.txt`; the test reads
it. Run the generator first, then the test with `KASSENSTURZ_BLESS=1` writes the
answer sheets and `flower-utopia.json` again. Python 3.9 or newer, standard
library only; the same call always writes the same files.

## The statements

Every book has one CSV file and one CAMT.053 file with the same nine lines. One
line is already in the book, the other eight are new (dated in the reference
month). A CAMT line carries the bank's reference, so it is found as *already
in the book*; a CSV line has none, so it is shown as *looks the same* and the
user decides (D-092). `<book>.csv.settings` holds the CSV settings the command
line takes; in the app the user maps the columns Buchungstag, Valuta, Name
Zahlungsbeteiligter, IBAN Zahlungsbeteiligter, Verwendungszweck, Betrag and
Währung.

## The answer sheets

`answers/<book>.md` is written by the pipeline test `example_books`
(`crates/kassensturz-pipeline/tests/example_books.rs`) from the real KMyMoney
importer, the real report code (last month's food spending, income and
expenses) and the real statement import with its duplicate check and balance
check. Nobody writes them by hand. The test fails if a book, a statement or a
sheet changes without the others, and it checks that every book has food
spending last month and at least one Transfer.
