# The five-year Space Colony book

`crates/kassensturz-import/tests/books/space-colony-5y-v15.sqlite` is a
made-up KMyMoney book (SQLite, layout 15) with 3,460 entries from
2021-11-01 to 2026-10-28, for the last local-model test round and the first
user tests (D-270). Open it in KassenSturz with *Settings › Start a book from a file…*;
the importer reads it like any KMyMoney book. Everything in it is invented:
no real person, bank or amount (AGENTS.md §3.1 rule 2).

## Make it again

```sh
tools/space-colony/generate.py            # writes the committed book
tools/space-colony/generate.py --check    # fails if the committed book differs
```

Python 3.9 or newer, standard library only. The script reads the older
`space-colony-v15.sqlite` for its accounts, categories, institutions,
securities and payees (its entries are dropped), adds the new payees, a
US dollar account and the dollar prices, and writes the entries. The same
call always writes the same content. Money is whole cents everywhere; there is
no floating point. Change an amount or a rule in the script, run it, run
`cargo test -p kassensturz-ai-tools --features ai --test space_colony_5y`,
and note any pinned answer that moved (the test names the questionnaire
questions) in the local-model questionnaire.

## What is in it

| Where | What | Good for trying |
|---|---|---|
| Everyday Checking | pay slip every month (five parts), rent and air recycling invoice (split), power, water, telecom, insurance, tuition, tax installments, card payments, a bank id on every line from mid 2022, reconciled to 2026-08-31 | totals, search, sorting, splits |
| Mars Rainy-Day Savings, Shuttle Travel Fund, Dome Petty Cash | monthly transfers, quarterly interest, flights paid from the travel fund | moves between own accounts (never categorized by the AI) |
| Shipyard Loan | 15,000.00 at the start, 180.00 repaid every month, 4,200.00 left | a debt in the account list |
| Earth Trip Wallet (USD) | seven trips: dollars bought in two transfers, then hotel, meals, tours and souvenirs in dollars | a second currency, `ai-tools.other-currency`, category totals in two currencies |
| The newest 16 lines of Everyday Checking | what a bank delivered in September and October and nobody has sorted | "Suggest categories", categorizing one entry |
| Payees and Rules | 33 Rules, none for Kepler Sports Club | the questionnaire's Rule question |

Prices rise a few percent a year, energy costs follow the seasons, and
December is dearer. Salary has a raise every January. The book ends today
(2026-10-28), so "all of 2026" and "up to today" give the same answer.

## Questionnaire answers that differ from the older book

The local-model questionnaire (ten questions and three bonus ones) was
written against the older book. Its answers for this book are pinned in
`crates/kassensturz-ai-tools/tests/space_colony_5y.rs`
(`the_questionnaire_answers_of_this_book`, `the_balances_are_the_generators`),
so a change to the generator that moves one fails there. App's today is
2026-10-28 or later.

| Question | Older book | Five-year book |
|---|---|---|
| 1 Spent in 2026, biggest categories | 1,353.12 EUR; Earth-Mars Flights 515.00, Everyday Spending 458.72, Olympus Mons Tours 90.00 | 44,878.94 EUR in the euro row and 2,433.53 USD in a second row; Colony Income Tax 10,230.00, Rent 7,350.00, Earth-Mars Flights 2,779.43, Hydroponic Greens 2,667.07, Colony Pension Fund 2,325.00; 532.90 without a category |
| 2 "Northern Ice" | 123 matches | 60 matches (20 shown, 40 left out) |
| 3 Everyday Checking | 5,531.90 EUR | 7,432.04 EUR |
| 4 List the categories | 70 | the same 70, same order |
| 6 Ten biggest expenses of 2026 | 555.00, 310.00, 150.00, 150.00, 148.50, … | -2,779.43, -1,240.00, -1,109.20, -988.75, -833.85, -831.08, -830.02, -827.95, -825.01, -822.18 (the eleventh is -821.04, so no tie) |
| 9 Food in 2026 | 153.51 EUR | 5,074.09 EUR: Food 1,734.03, Hydroponic Greens 2,667.07, Algae Bars and Nutrient Paste 672.99 |
| 5, 7, 8, 10 to 13 | — | unchanged (Pets absent, no Rule for Kepler Sports Club, Leisure › Zero-G Sports exists, Transactions › Change off refuses) |
