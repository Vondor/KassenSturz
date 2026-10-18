# Proposal: number format for exchange rates, prices and quantities

**Status:** Decided on 2026-10-18: option B, implemented with `rust_decimal` (D-051 in `docs/DECISIONS.md`). Kept as the background for that decision.

## Why this needs deciding before M1

Hard rule 1 (AGENTS.md §3.1) rules out floating point for money, prices, exchange rates and quantities. Amounts already have an exact format: 64-bit integers in minor units (D-027). Exchange rates, security prices and share quantities need an exact format too, and M1 implements currencies and rates.

## What the format has to handle

- **Exchange rates** such as 1 EUR = 1.0856 USD, and fixed conversion rates with six significant digits such as 1 EUR = 1.95583 BGN.
- **Prices and quantities** once investments arrive (after 1.0, D-029). Fund savings plans buy fractional shares such as 0.1234. Cryptocurrencies, if they come later, are tracked like securities (D-027) and use up to 18 decimal places.
- **KMyMoney import** (D-003). KMyMoney stores these values as fractions (numerator/denominator). Most come from decimal input, but a price computed from a purchase can have any denominator, e.g. 2500/37.
- **Conversions that produce amounts:** amount × rate, rounded once to the target currency's minor units with the book's rounding mode (D-027). Converting back divides by the same rate, so no inverse rate needs to be stored.
- **Storage in SQLite**, whose only exact numeric type is the 64-bit INTEGER. REAL is floating point and therefore ruled out.

## The two options

| | A) Exact fractions | B) Fixed-point decimals |
|---|---|---|
| Stored as | numerator and denominator (two integers) | digits and number of decimal places (e.g. 195583 and 5) |
| Exact for decimal input (bank files, quoted rates, user entry) | yes | yes |
| Exact for division results (1/3, price = amount ÷ quantity) | yes | no, rounded at a fixed precision |
| KMyMoney import | lossless | lossless for decimal values; a computed price like 2500/37 is rounded and listed in the import report (D-029) |
| Showing a value | needs converting to decimals; 1/3 can't be shown exactly | shown as stored |
| Comparing and sorting, also in SQL | needs cross-multiplication | direct |
| Arithmetic pitfalls | denominators grow, so values need reducing and an overflow strategy | few: align the decimal places, compute in 128-bit |
| Ready-made crate | `num-rational` (MIT or Apache-2.0) | `rust_decimal` (MIT, up to 28 significant digits) |

Either way, money stays exact: every amount is still rounded exactly once, to minor units, by an explicit rule. The options differ in how much complexity sits in the numbers that lead to an amount.

## Recommendation: B, fixed-point decimals

- It matches how rates and prices are quoted, typed and shown, so what the user enters is exactly what is stored.
- The arithmetic stays simple to implement and to cover with property-based tests (AGENTS.md §9).
- Its one inexact case, division, never reaches a stored amount except through the single explicit rounding: conversions compute amount × rate or amount ÷ rate in 128-bit integers and round once (D-027). A price derived from a purchase only feeds valuations, and rounding it at 12 decimal places moves a valuation by far less than a cent.
- Storage: the digits and the number of decimal places as two INTEGER columns. A 64-bit integer holds any 18-digit number, which is enough for rates, prices and fund quantities; cryptocurrencies, if they come, may need a wider column then.
- Implementation: either a small type of our own on 128-bit integers, or `rust_decimal` wrapped in KassenSturz's own types (`ExchangeRate`, `Price`, `Quantity`) so that its conversions to and from floating point never appear in our API. `rust_decimal` would be a direct dependency and needs your approval (D-026).

## What you need to decide

1. A (exact fractions) or B (fixed-point decimals).
2. If B: a type of our own, or `rust_decimal`.
