# Glossary

Plain-language terms used in the UI, and the accounting/technical concept each one maps to. Per AGENTS.md §6: no accounting jargon in the default UI — say what a thing does, not what it's called in a textbook. Keep this updated as user-facing terms grow. Rows marked *(proposed)* await the maintainer's approval. German terms are given where suggested, since v1 ships in English and German (D-034).

| UI term | Means (technical/internal concept) |
|---|---|
| Account | A place where your money is: a checking account, a savings account, a credit card, or the cash in your wallet. German: *Konto*. Internal concept: an asset or liability account. Not the same thing as a Member (D-022). |
| Category | What money was spent on or came from, e.g. Groceries, Rent, Salary. New users start with a built-in set (D-029). German: *Kategorie*. Internal concept: an income or expense account. |
| Household book | Everything KassenSturz keeps about your money, in one place: accounts, categories, transactions, Rules. German: *Haushaltsbuch*. Internal concept: the book (one SQLite file with the extension `.ksz` in v1, D-033, D-037). |
| Starting balance | How much was in an account on the day you started tracking it in KassenSturz. German: *Anfangsbestand*. Internal concept: the opening balance, booked against an internal equity account the user never sees. |
| You owe | How a debt is shown, e.g. a credit card balance: "You owe €300" instead of "−€300" (D-067). Internal concept: the balance of a liability account. |
| Rule | A user-defined condition-and-action pair that auto-categorizes transactions, modeled on email-client filters. Internal concept: `Rule` (conditions, actions, optional sub-Rules forming a tree). See D-014 in `docs/DECISIONS.md`. |
| Sub-Rule | A Rule nested under another. Only checked once its parent already matched; if it also matches, it refines/overrides the parent's result. |
| Pattern matching (advanced) | Regular expressions (regex), offered as an opt-in per Rule condition for users who want it — never required for basic use. See D-015. |
| From / To | The other side of a transaction: shown as **From** on money you receive and **To** on money you pay. In the Rule editor the field is called **Other party**. Replaces the earlier term "Sender", which was wrong for payments. Internal concept: the payee/counterparty (name and IBAN). See D-034. |
| Part | One piece of a transaction broken down by what it was actually for (e.g. one €53.20 grocery charge split into Food, Household, Fast food Parts). Internal concept: `Split`. See D-017. |
| Break down (a purchase) | The action of adding/editing a transaction's Parts. Available any time, not just at import. |
| Scan a receipt | Post-1.0: photograph (Android companion app) or import an existing scan/photo of a receipt, so KassenSturz can suggest which transaction it belongs to and propose its Parts. Internal concept: `kassensturz-receipts` pipeline. See D-018. |
| Transfer | Moving money between two of your own accounts (e.g. Checking → Savings). Never counted as income or spending. Internal concept: a transaction whose splits are both real (asset/liability) accounts, no income/expense category. See D-021. |
| Member | Post-1.0: a person using KassenSturz (e.g. each spouse in a household) — **not** the same thing as a financial Account; don't conflate the two. A Member sees only Accounts they own plus Accounts explicitly shared with them. See D-022. |
| AI memory | What the AI feature remembers about this book (its setup, and recurring patterns like "this payee is usually categorized as X") so it can be more useful without being re-explained every time. Never used to store actual balances or totals — those always come from code. Viewable, editable and deletable by the user; off entirely when AI is off. Comes after 1.0 (D-071). See D-023. |
