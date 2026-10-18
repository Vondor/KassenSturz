# Glossary

Plain-language terms used in the UI, and the accounting/technical concept each one maps to. Per AGENTS.md §6: no accounting jargon in the default UI — say what a thing does, not what it's called in a textbook. Keep this updated as user-facing terms grow.

| UI term | Means (technical/internal concept) |
|---|---|
| Rule | A user-defined condition-and-action pair that auto-categorizes transactions, modeled on email-client filters. Internal concept: `Rule` (conditions, actions, optional sub-Rules forming a tree). See D-014 in `docs/DECISIONS.md`. |
| Sub-Rule | A Rule nested under another. Only checked once its parent already matched; if it also matches, it refines/overrides the parent's result. |
| Pattern matching (advanced) | Regular expressions (regex), offered as an opt-in per Rule condition for users who want it — never required for basic use. See D-015. |
| Sender | The payee/counterparty name on a transaction (who sent or received the money). |
| Part | One piece of a transaction broken down by what it was actually for (e.g. one €53.20 grocery charge split into Food, Household, Fast food Parts). Internal concept: `Split`. See D-017. |
| Break down (a purchase) | The action of adding/editing a transaction's Parts. Available any time, not just at import. |
| Scan a receipt | Post-1.0: photograph (Android companion app) or import an existing scan/photo of a receipt, so Kashflow can suggest which transaction it belongs to and propose its Parts. Internal concept: `kashflow-receipts` pipeline. See D-018. |
| Transfer | Moving money between two of your own accounts (e.g. Checking → Savings). Never counted as income or spending. Internal concept: a transaction whose splits are both real (asset/liability) accounts, no income/expense category. See D-021. |
| Member | Post-1.0: a person using Kashflow (e.g. each spouse in a household) — **not** the same thing as a financial Account; don't conflate the two. A Member sees only Accounts they own plus Accounts explicitly shared with them. See D-022. |
| AI memory | What the AI feature remembers about this book (its setup, and recurring patterns like "this payee is usually categorized as X") so it can be more useful without being re-explained every time. Never used to store actual balances or totals — those always come from code. Viewable, editable and deletable by the user; off entirely when AI is off. See D-023. |
