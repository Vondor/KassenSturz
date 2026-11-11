# 15. Formulas and patterns

*A formula calculates with your categories, like a spreadsheet. This page shows what is possible, with made-up examples. Patterns are only for Rules, and only for people who want to be more exact.*

## Where you write formulas

In the **Properties** of a **One figure** or a **Progress bar** (and in the Statistic Builder under **Advanced**). The result is shown next to the formula, so you never have to trust it blindly. Errors say in words what is wrong ("Travel is neither a category nor a group").

## Names

A name stands for a category or one of your groups and gives its total in the figure's period, as a positive amount (a refund makes it smaller). Everything inside it counts too.

- A simple name: `Groceries`.
- With spaces or special characters, in single quotes: `'Eating out'`.
- A path for a category inside another one: `'Everyday > Pets'`.
- If there is a money-in and a money-out category with the same name, put `Income.` or `Spending.` in front: `Spending.Pets`.
- **Everything spent, everything received:** `Spending.ALL` and `Income.ALL`, including what isn't sorted yet.

Accounts are not names. All names in one formula must have **the same currency**.

## Calculating

`+`, `-`, `*`, `/`, brackets and `%`. Example: what is left of the salary after the rent:

`=Salary - Rent`

An amount divided by an amount is a number (a ratio); KassenSturz refuses to multiply two amounts. Everything is calculated with exact decimal numbers; **only division rounds** (to 10 places), and the result is rounded once at the end. Dividing by zero gives **no value**, never zero.

## A name's period

Write `@` and a period after the name:

- Words in double quotes: `"this month"`, `"last month"`, `"this quarter"`, `"last quarter"`, `"this year"`, `"last year"`, `"last 12 months"`, `"last 3 months"`, `"last 3 complete months"` (the number from 1 to 120). The period words are always written in English, like the function names.
- Dates: `@2026` (a year), `@2026-03` (a month), `@2026-01..2026-03` (months from and to), `@2026-01-15..2026-02-14` (days from and to).

Examples:

- Average of the last three complete months: `=Groceries@"last 3 complete months" / 3`
- Rent over the last six months: `=Rent@"last 6 months"`

The periods count from the figure's period, not from today: if the page shows March, `"last month"` is February.

## Functions

The function names are English and the same in every language; in the editor, a list explains each one in a sentence.

| Function | Gives |
|---|---|
| `SUM(a, b, …)` | the total |
| `AVERAGE(a, b, …)` | the average |
| `MIN(…)`, `MAX(…)` | the smallest, the largest value |
| `COUNT(name, …)` | how many transactions in the period belong to the categories (each one once) |
| `ABS(x)`, `SIGN(x)` | without a sign; −1, 0 or 1 |
| `ROUND(x, places)`, `ROUNDUP`, `ROUNDDOWN` | rounded (half away from zero, away from zero, towards zero) |
| `INT(x)` | rounded down to a whole number |
| `IF(test, yes, no)` | one or the other, depending on the test |
| `AND`, `OR`, `NOT` | true or false, for `IF` |
| `IFERROR(x, instead)` | `instead` if `x` has no value |
| `PERCENT_OF(part, whole)` | the share in per cent |
| `CHANGE(new, old)` | the change in per cent |

With an English number format, the separator between the parts is `,` and the decimal mark is `.` (`=ROUND(Pets * 1.5, 2)`). `;` is accepted in every number format, so a formula copied from German help text still works (`=ROUND(Pets * 1.5; 2)`).

Examples:

- What per cent of spending is groceries? `=PERCENT_OF(Groceries, Spending.ALL)`
- Has travel changed? `=CHANGE(Travel, Travel@"last year")`
- Only a surplus, never a minus: `=IF(Salary - Rent > 0, Salary - Rent, 0)`

## Formulas and lists of your own

The lists "Money in: only these …" do **not** apply to formulas: a name in a formula counts the accounts and categories of the whole chart. The help in the Properties says so too.

## Patterns in Rules

In a condition of a Rule (page 12) you can choose **Use a pattern (advanced)**. It then finds texts that have a certain shape, instead of only one word. You never need it. How to write patterns and formulas is explained on the help page **Help: patterns and formulas** in the app, with made-up examples. It opens from **Settings** (the "About" card) and from **More about patterns** under a pattern condition in a Rule.

**If something looks different:** the functions on this page are those of the current version. If a message says a function is unknown, it isn't listed here and doesn't exist yet.
