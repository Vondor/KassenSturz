# How safe is KassenSturz?

*For anyone who wants to know whether they can trust KassenSturz with their bank data. No jargon. This page is also available [in German](SECURITY_OVERVIEW.de.md), which is the version for testers.*

In short: your data stays on your computer, KassenSturz only reads from your bank and never sends money anywhere, and we attack the app ourselves to find mistakes before you do. KassenSturz is still in an early test stage, though. Further down we say honestly what it can **not** do yet.

## Where your data is

- **On your computer, nowhere else.** Your household book is a file on your computer. There is no online service and no account with us. We never see your data.
- **No hidden connections.** KassenSturz sends no usage data, no crash reports and no ads. It connects only when you start it yourself: to your bank, or to an AI you switched on and chose yourself.
- **Your data is yours.** You can copy and export the book at any time, in an open format.

## Your bank and your PIN

- **Read only, never send.** KassenSturz fetches balances and statements. It cannot start transfers, standing orders or direct debits; that is left out on purpose.
- **Your PIN is in no file.** You decide whether KassenSturz asks for the PIN every time, keeps it in memory until the app closes, or stores it in your system's password store. Neither the PIN nor the login name is in the book, in backups or in copies.
- **Protection against a locked account.** When the bank says the PIN is wrong, KassenSturz forgets it at once and asks once more. After a second wrong PIN it stops, so your access isn't blocked.
- **Encrypted connection.** The connection to the bank is encrypted. If the bank's security certificate doesn't check out, KassenSturz stops. There is no "continue anyway".
- Nobody from KassenSturz will ever ask you for your PIN or a TAN.

## The AI is optional

- **Off until you switch it on.** While AI is off you see nothing of it, and nothing of it runs in the background.
- **You choose where.** You can use an AI model on your own computer. Before anything leaves your computer, KassenSturz says what will be sent and asks you.
- **The AI proposes, you decide.** You see every change an AI prepares and confirm it yourself. Large or deleting changes get a clear warning.
- **KassenSturz does the sums, not the AI.** Balances and totals always come from the program itself.

## Protection against losing data

- **Everything can be undone.** Every change, even a whole imported statement, can be taken back in one step.
- **All or nothing.** If the computer goes off in the middle of a save, afterwards either everything is saved or nothing is. There are no half-saved transactions.
- **A backup before every update.** Before a new version converts your book, it makes a backup copy.
- **A check at every open.** A book into which someone has secretly built their own commands is recognised and not opened.

## What we attacked

We treat KassenSturz the way an attacker would. Part of the attacks is built into the automated tests and runs on every change to the program. On top of that come attack rounds in an isolated test computer, partly by hand, partly by AI models told to fool the app on purpose. Always with made-up example books, never with real data.

Among other things we attacked:

- **Forged and broken files:** statements (MT940, CAMT, OFX, CSV) and KMyMoney files with wrong data, impossible amounts, hidden characters and deliberately huge content. Also a random test with many millions of mangled files, and a test that recomputes every balance to the cent after more than a million imported entries.
- **Money:** huge amounts, rounding between currencies, split transactions, transfers, planned payments and the forecast.
- **Crashes and power cuts:** the program killed in the middle of saving, importing or undoing, thousands of times; real power cuts in the test computer; a full disk; a USB stick that suddenly disappears; a clock that jumps forward or back.
- **Two programs on the same book:** several windows and programs writing at the same time.
- **Fooling with names:** accounts and categories that look the same but contain different, invisible or foreign letters.
- **Fooling the AI:** payee names and notes with hidden commands to the AI, tested with several local AI models. None of them followed the commands, and every change waited for the user's confirmation.
- **"AI off" really off:** whether anything of the AI still runs or shows while it is switched off.
- **Export:** whether a name in an exported table can turn into a dangerous formula in a spreadsheet.

## What we found and fixed

Every finding gets a number (B-…) and is fixed in its own change (#…). Afterwards we attack exactly that spot again.

| What could happen | Finding | Fixed in |
|---|---|---|
| A deliberately altered book could delete parts of transactions on an edit, and Undo could write back wrong amounts | B-245, B-246 | #883 |
| An AI could propose more changes at once than allowed, and names with foreign look-alike letters went unnoticed | B-247, B-250 | #884 |
| Large, deleting AI changes came without a clear warning; Rules made by an AI weren't marked as such | B-248, B-249 | #900 |
| A web address in an AI answer reached the user without a note | B-251 | #888 |
| A consent written into the settings file by hand counted like the user's real one, so data could have gone to an AI elsewhere | B-252 | #897 |
| A chat transcript stayed on disk after the developer mode was switched off | B-253 | #885 |
| Very large amounts could make balances unreadable | B-269 | #979 |
| A transfer from an account to the same account was accepted | B-270 | #1002 |
| For very long periods the list of planned payments wrongly said "none planned" | B-271 | #999 |
| An invisible character turned "Giro" into a second account that looked the same | B-272 | #1005 |
| A clock set far ahead deleted the whole change history | B-274 | #975 |
| When the system ended the app, the book stayed marked as "open" | B-276, B-287 | #989, #998 |
| On import, two real identical entries (for example two coffees) were counted as one | B-277, B-279 | #986 |
| A power cut shortly after saving could take back the last change | B-278 | #983 |
| Some statements in other character sets weren't read | B-280 | #996 |
| With some CSV files the first entry was lost as a header | B-281 | #984 |
| A KMyMoney book with two accounts of the same name couldn't be imported | B-283 | #997 |
| A receipt swapped in the receipt folder went unnoticed | B-284 | #969 |
| After going back to an older version, the pointer to the backup was missing | B-285 | #992 |
| With AI off, an invisible remnant of the chat window still existed | B-286 | #991 |
| Too-large amounts in AI proposals were refused late and with the wrong reason | B-288 | #994 |
| When a sync program replaced the open book, the message was unclear | B-289 | #1000 |
| A notice about very many reports was cut off | B-290 | #1001, #1003 |
| The Windows package held 131 programs nobody needs (such as ffmpeg and openssl), each one more that could be misused | B-292 | #1014 |
| When the drive was full, an import ended with a message that said neither what happened nor what to do | B-282 | #1007 |
| A book with one changed number in its file opened with a wrong balance, and a damaged page only failed at the next save | B-291 | #1010 |
| A part pointing to a missing account counted in no balance, and nothing said so | B-294 | #1020 |
| An index of the book that disagreed with its table could hide entries from a balance | B-295 | #1020 |

No finding revealed a PIN. Only one (B-252) could have let data leave the computer, and only if someone had first forged the settings file by hand.

## What KassenSturz can't do yet (honestly)

- **It is a test version.** KassenSturz is in its Alpha stage. For testing, please use made-up numbers or an example book, not your real household book.
- **No review by an outside security firm.** KassenSturz is built and checked mostly by AI programs, supervised by a person who makes every important decision. A focused extra review of the most sensitive parts (money, password store, bank) is planned before the finished version.
- **The book is not encrypted.** It holds no PIN, but it holds your transactions. Whoever gets a copy of the file can read it. Protect your computer with a password and, if you can, an encrypted disk; the same goes for backups and sync folders.
- **Malware on your computer** is something KassenSturz can't defend against. A program running as you can read your files. Keep your system up to date.
- **The snap package on Linux:** in some folders the system doesn't let the snap version lock the book against being opened twice. KassenSturz tells you when that happens. Then don't open the same book in two programs at once.
- **Windows** has been tested mostly automatically so far; tests on a real Windows computer are starting now.
- **Translations** were made with AI and are being checked by people step by step. If a text is unclear, please tell us.
- **A few smaller findings are still being worked on**, for example a clearer message when the disk is full. As far as we know, none of them puts your data at risk.

## If you notice something

- **A security problem:** please don't report it publicly; report it privately as described in [SECURITY.md](../SECURITY.md).
- **Anything else:** through the person running your test, or as an issue on GitHub.
- Never send real statements, account numbers, PINs or TANs, not even in a screenshot.

---

*For developers and AI agents: how we attack and where every attack is recorded is in [SECURITY_LOG.md](SECURITY_LOG.md). The technical side is in [THREAT_MODEL.md](THREAT_MODEL.md).*
