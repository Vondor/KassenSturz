# Daily balance with averages and MACD (US7, #1036, D-794)

A dataset of the Statistics Builder, `daily_balance`: what the chosen
accounts held at the end of each day of a period, drawn as a stepped line,
with two exponential moving averages over it and, in a second panel under
it, MACD with its signal line and histogram. Each panel has an axis of its
own. Nothing on the start screen changes (D-342). The monthly moving
average and the monthly MACD stay as they are (`docs/TREND_LINES.md`).

## Rules

- **The balance of a day** is the one `Book::balance_on` gives for it (the
  same as the AI's `account_balance`): every entry counted on the day the
  bank booked it where it says so, otherwise on its date, starting balances
  included. Without a chosen account, the checking, savings and cash
  accounts are added up, as in the other balance datasets. Each currency is
  its own chart; currencies are never added together.
- **The days shown** end on the period's last day or today, whichever comes
  first, so the last day's balance is today's `account_balance`. A period
  wholly after today shows no days. At most `MAX_DAILY_DAYS` (1100, about
  three years) are shown; a longer period is refused with
  `report.period-too-long`.
- **Averages (EMA).** An average of `n` days weighs the newest day with
  `2 / (n + 1)`: each day's average is the day before's plus
  `2 × (balance − average the day before) / (n + 1)`, worked out in exact
  minor units and **rounded once a day** in the book's rounding mode
  (D-027). It starts at the first day's balance. No floating point anywhere;
  the screen gets text and only draws with coordinates made from it (D-065).
- **MACD** is the short average less the long one. The *signal line* is the
  average of MACD over `signal` days, started at the first day's MACD, and the
  *histogram* (the bars) is MACD less the signal line, so each row of the
  table adds up.
- **Warm-up.** The averages start `4 × (long + signal)` days before the
  period and are not shown for those days, so the first day shown already
  carries a settled average (the first day's weight is then below 0.05 % for
  every length offered).
- **Days before the first entry.** The averages never start before the
  chosen accounts' first entry: a day before it has no balance yet. If that
  first entry lies inside the period, the chart starts on that day and says
  so under the chart ("The chart starts on …, the day of the first entry").
- **Settings** (words in `params`, so no new setting kind is needed):
  - `ema_fast` (default `12-days`; also 5, 7, 9, 10, 14, 20, 50);
  - `ema_slow` (default `26-days`; also 14, 20, 30, 50, 90, 100, 200), and
    the long average always spans more days than the short one (a shorter
    one is taken as short + 1);
  - `ema_signal` (default `9-days`; also 3, 5, 7, 12, 14).
  They are written to a dashboard file only for this dataset, and the Builder
  shows them only there.
- **What is shown** is the grouping: `balance-averages-macd` (default, both
  panels), `balance-averages`, `balance` or `macd`. Line, area and bar charts
  and the table take it; round and stacked charts don't.
- **Hover** names the day, the balance, both averages, MACD, the signal line
  and the histogram, on either panel.
- **No total** (a total of balances means nothing).

## AI

`daily_balance` is `for_ai`. `propose_view` offers it in the `data` list with
its description, `group_by` takes the four words above, and `ema_fast`,
`ema_slow` and `ema_signal` set the lengths. The monthly `moving_average` and
`macd` take `trend_length`, `trend_slow` (MACD only) and `trend_signal` (MACD
only) the same way. A word that is not in its list is refused with the list
(`allowed`); a length the chosen data does not have is refused with a hint,
never silently dropped. Every figure is computed by KassenSturz (D-052).

The read function `daily_balances` gives the model the same figures for one
account (under *Accounts › Read*, like `account_balance`): `account_id`,
`from` and `to` (at most 200 days, days after today left out), and
optionally `ema_fast`, `ema_slow` and `ema_signal` as whole days from 2 to
200, the long average longer than the short one (default 12, 26, 9). Each
row is `day, balance, average_short, average_long, macd, signal, histogram`;
`starts_on_first_entry` says the account has no balance before
`first_day`, and a period wholly after today has no rows and says
`after_today`. Every amount goes through the number check, so a figure the
model makes up is flagged (T-23).
