<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC0-1.0
-->

# Moving averages and MACD (US7, #875, D-777)

Two datasets of the Statistics Builder, shown by the line, bar and table
types that already exist. The user picks the dataset like any other; nothing
on the start screen changes (D-342).

| Dataset | What it draws |
|---|---|
| `moving_average` | One of *what is left*, *money in*, *money out* (the grouping, default *what is left*) each month, and next to it its moving average over *N* months. |
| `macd` | The *trend* line (average of the last *N* months less the average of the last *slow* months), the *signal* line (average of the trend over the last *signal* months) and the *gap* (trend less signal). |

## Rules

- **Exact.** Months come from `income_expense_by_month`, so refunds, Transfers, starting balances and currencies are counted as there. An average is the exact `i128` sum of the month and the ones before it, divided **once** by the number of months in the book's rounding mode (D-027). The gap is the difference of the two rounded figures beside it, so a table always adds up.
- **Warm-up.** The months an average needs before the period (`Trend::read_span`) are read and counted **in full**, and not shown, so the first month shown already has its whole window. The first month shown also counts in full, not only from the period's first day. Months before the first transaction count as zero, as in D-668.
- **Settings** (`params`, words so no new setting kind is needed): `trend_length` (default *3 months*), `trend_slow` (*6*), `trend_signal` (*3*). The long average always spans more months than the short one. They are written to a dashboard file only for these two datasets, and the Builder shows them only there (*Long-term average over* and *Signal line over* only for MACD).
- **Names for everyday users.** *Moving average*, *Trend*, *Signal line*, *Gap*; the explanation in the Builder says in two sentences what each shows. No chart total (a total of averages means nothing).
- **AI.** Both datasets are `for_ai`: the AI may propose them through `propose_view` with the default lengths; the user changes the lengths in the Builder. Figures are computed by code only.
- **Custom charts** and the formula system do not read these datasets (no table of fields).
