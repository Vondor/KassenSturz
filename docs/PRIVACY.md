# Privacy policy

*Applies to KassenSturz, the free household-budget app. Last checked against the code: 17 November 2026. German version: [PRIVACY.de.md](PRIVACY.de.md).*

## In short

- **No account, no telemetry.** KassenSturz has no sign-up, no usage statistics, no crash reports, no ads and no update check. The project runs no server that your data could reach.
- **Your data stays on your computer.** Your household book is a file you choose the place of. The developers never see it.
- **Nothing is sent unless you switch something on yourself.** Only two features can connect to the internet or to another computer, and both are off until you use them: the bank connection and the AI.

## What KassenSturz stores, and where

- Your accounts, transactions, categories, plans and settings, in your book file and in backup copies of it next to the book, on your own computer.
- If you choose, your bank PIN in your system's password store (Windows Credential Manager, KWallet, GNOME Keyring or KeePassXC). It is never written into the book or a backup. By default it is asked each time or kept in memory until the app closes.

None of this is sent to the project, to SerpentSoftware or to anyone else by KassenSturz.

## When something leaves your computer

1. **Bank connection (optional).** If you set up a bank, KassenSturz connects to **your bank** (FinTS/HBCI, encrypted) to read balances and statements. It never sends payments. The connection goes only to your bank. Your bank has its own privacy policy.
2. **AI (optional, off by default).** If you switch AI on and enter an AI provider, KassenSturz sends the texts the AI needs to answer you (for example payee names, notes or amounts you asked about) **only to the provider you chose**, and only after you agreed once, with a statement of what is sent. If you choose a model on your own computer, nothing leaves it; a model on another computer counts as leaving and asks for your agreement. While AI is off, no AI code runs and nothing AI-related opens a connection.

The app's own screens cannot load anything from the internet. Opening a link (for example the project's web page) is done by your browser, because you clicked it.

## What we do not do

We do not collect, sell or share personal data, we do not profile you, and we do not use your data for advertising. We do not ask for your PIN or a TAN, and nobody from the project will.

## Your control

Delete the book file and its backups and nothing remains. Export your data at any time in an open format. Switch AI off in Settings and it disappears from the app. Do not set up a bank and KassenSturz never connects to one.

## Other places

If you report a problem on GitHub or get the app from a store, that service's own policy applies to that service (for example GitHub, Microsoft or Canonical). Do not put real financial data into a report.

## Changes and contact

If this ever changes, this page changes first, in the project's public history. Questions: open an issue at <https://github.com/Vondor/KassenSturz/issues>. The facts above are checked against the code and the [threat model](THREAT_MODEL.md); [How safe is KassenSturz?](SECURITY_OVERVIEW.en.md) gives more detail.
