# Notes for the user guide

What the user sees and does, step by step, and the messages they can meet. Each M3 step adds a few lines here, so the handbook can be written from them later.

## Your book (M3.1)

- **At the first start** KassenSturz makes your household book by itself, in its own folder, and opens it every time after that. You don't have to choose a file or know where it is.
- **Settings → Your book** shows the book's name and folder. "Open another book…" and "Start a new book…" are there for people who want more than one book, e.g. to try something out; most households need only one. KassenSturz then opens the book you used last.
- **Backups:** before KassenSturz updates a book to work with a new version, it saves a copy in the backup folder shown in Settings, and keeps the last 10 copies of each book.
- **"This book is open on Laptop":** a book is open in one place at a time. Close it on the other computer first, then choose "Try again". If KassenSturz isn't running there anymore, e.g. after a crash, "Open anyway" opens it here.
- **"This book is still marked as open on this computer":** KassenSturz may be open in another window, or it didn't close properly last time. If it isn't open anymore, choose "Open anyway".
- **"KassenSturz can't find your book":** the book you used last was moved, renamed, or is on a drive that isn't connected. Connect the drive and choose "Try again", or open the book where it is now. KassenSturz never makes a new, empty book in its place on its own.
- **Network drives:** a book on a network drive (e.g. a NAS share) opens with a warning, since network drives can damage a book. Keep it on your computer; a sync folder such as Nextcloud's is fine.
- **In the Flatpak** KassenSturz can't open books outside its own folder yet, and says so (Q-071).

## How amounts look (M3.2)

- **Settings → How amounts look** starts with your regional settings, e.g. 1.234,56 € in Germany. You can change the decimal separator, the thousands separator, whether the currency symbol comes before or after the amount, whether amounts below zero have a minus sign or parentheses, and whether decimal places are shown. The example at the top shows the result straight away.
- **Choosing a separator the other one uses** swaps them: with a decimal point in Germany, thousands are separated by commas, so an amount can't be misread.
- **Rounded amounts:** without decimal places, 53,60 € shows as 54 €. Only the display is rounded: KassenSturz adds up the exact amounts first, so rounded figures may not add up exactly on screen. Point at an amount to see it exactly.
- **Typing amounts** follows the same separators, and you can always type all the decimal places.
- These settings belong to this computer, not to the book: someone else opening the same book sees amounts their way.

## Accounts and categories (M3.3)

- **Accounts** (side menu) shows your accounts side by side with what is in each, in the order you added them. A credit card reads "You owe 300 €". From two accounts on, "All together" shows what you have minus what you owe. If you have accounts in more than one currency, each currency has its own group and its own total: KassenSturz never adds euros and dollars together.
- **Add account:** choose what kind it is, give it a name, say how much is in it and on which day. The day is today unless you change it, and the amount is what the account held at the end of that day. Leave the amount empty if there is nothing in it. "This account is not in EUR" (or whichever currency you use) lets you choose another one, by its code.
- **Open an account** by clicking its card: rename it, check it against your bank, or close it. You can close an account when nothing is left in it, so no money is hidden; it keeps its history and you can show closed accounts and open one again.
- **Check against my bank:** type what your bank says is in the account on a day. KassenSturz says whether it agrees. If not, it shows the difference exactly and where it can come from: an entry that may be in twice or the wrong way round, an entry that may be missing, entries or a starting balance dated after that day, and what the starting balance would have to be for both to agree. It changes nothing by itself; it offers to correct the starting balance, see "The first start".
- **Messages:** "This day hasn't come yet" (type today or an earlier day); "This account still has 100,00 € in it. Move the money to another account first, then close it."
- **Categories** (side menu) lists where money comes from, then what it is spent on, with categories inside categories indented, e.g. Groceries inside Food. Click one to rename it or move it, with everything inside it, into another category or to the top level. "Add category" makes a new one, on its own or inside another. Categories can't be deleted yet.

## The first start (M3.4)

- **Welcome:** the first screen says what KassenSturz does and asks which currency you use. Your region's currency is already chosen, so most people only confirm; you can add an account in another currency later. "Add your first account" takes you on.
- **Your first account:** choose what kind it is, give it a name, and say how much is in it and on which day, as on "Add account". Take the balance from your bank's statement or app, so KassenSturz starts from what you really have.
- **Your first transaction:** add something you paid or received, for example your last shopping trip, or "Skip for now". Then the overview appears and the side menu is there.
- **Categories to begin with:** KassenSturz gives you a set in your language: where money comes from (Salary, Pension, Benefits, Interest, Gifts received, Money back, Other income) and what it goes on (Home, Food, Getting around, Health, Insurance, Clothes, Children, Pets, Free time, Education, Gifts and donations, Fees and charges, Other spending), several with categories inside, e.g. Groceries and Eating out inside Food. They are yours: rename, move or add to them under Categories.
- **Correcting the starting balance:** when "Check against my bank" finds a difference, it also offers "Set the starting balance to 800,00 €" (or "Add a starting balance of …" if the account has none yet). That is for a difference from before you started using KassenSturz, e.g. after you imported older entries. Click it and the account holds on that day what your bank says; your other entries stay as they are, and Undo takes it back. KassenSturz never changes the starting balance on its own.
- **Messages:** "The starting balance is dated after this day, so it can't be corrected from here. Check a day on or after the starting balance."
- **What your bank said (M3.4a):** the account's page remembers the balance you took from your bank and the day: "On 1 Oct 2026 your bank said 1.000,00 €, and KassenSturz agrees." If you later add an entry dated before that day, the account no longer holds what the bank said, and the page says so: "On 1 Oct 2026 your bank said 1.000,00 €, but KassenSturz now has 975,00 € for that day. An entry dated before that day may have changed it. Check against your bank to put it right." Its button opens "Check against my bank" with that balance and day already filled in, compared, and the correction offered. Correcting the starting balance from a check also updates what the page remembers to the balance and day you checked. Accounts added without a balance, and accounts from before this, remember nothing, and the page shows nothing about it.

## Transactions (M3.5)

- **Show transactions** on an account's page, or a click on an account card on the overview, lists everything that went into and out of that account, newest first. Each line says who the money came from ("From: Example Employer Ltd.") or went to ("To: Corner Grocery"), the date and the category, the amount, and what was in the account after it ("Then: 946,80 €"; on a credit card "Then you owed: 389,90 €"). The balance is at the bottom.
- **Sorted by** in the toolbar changes the order: date, amount, from or to, or category, and turns it around ("Oldest first", "Smallest first", "Z to A"). The "Then" amounts don't change with the order: they always count in date order.
- **Add transaction** on the list adds to that account; on the overview you choose the account first if you have more than one. Say what happened (paid or received), how much, from or to whom, what it was for and when. The amount is typed the way your settings show amounts, e.g. 53,20 in Germany. "Not sorted yet" leaves the category open; you can sort it later.
- **Click a transaction** to change any of this or remove it. Removing takes it out of the list and the balance; Undo brings it back.
- **What can't be changed on this page:** the starting balance ("To change it, check the account against your bank"; the button takes you there) and money moved between two of your accounts, which can be removed and typed again as its two sides (see Moving money between your accounts). A transaction broken down into several parts opens its own page (see Parts). An entry that came from your bank keeps its amount as the bank sent it; the rest can be changed.
- **Messages:** "Type an amount."; "This isn't an amount. Type it like this: 1.234,56"; "Type an amount above zero. Whether you paid or received it, you choose above."; "This isn't a date. Type it like this: 19.10.2026"; "This entry came from your bank or a file, so its amount stays as it was delivered. You can change the rest."; "This transaction isn't in the book anymore. It may have just been removed."

## Parts (M3.6)

- **Break down this transaction…** at the bottom of a transaction's page opens the page where you say what each part of it was for: for example one grocery bill as 40 € Groceries, 8 € Household goods and 5,20 € Eating out. A transaction that already has parts opens this page directly from the list. **Add a part** adds a line; the button at the end of a line removes it.
- **One part's amount can be left empty.** KassenSturz works it out from the total and the other parts and shows it in the empty field ("Part 3 gets what is left: 5,20"). When you open a transaction, its last part is shown that way, so you can add parts and type their amounts, and the last one shrinks by itself.
- **It has to add up.** Below the parts the page says "The parts add up to 53,20 €, the whole transaction." or what is still to be sorted or too much. If every amount is typed and they don't add up, saving is refused and the message says by how much. A part can have a minus sign, e.g. a deposit paid back on the same bill.
- **A part in another currency:** a category that counts in another currency is listed with it, e.g. "Travel (USD)". Such a part gets a second field, "In USD:", for what it was in that currency, e.g. from the receipt, and the page shows the rate the two amounts make ("1 EUR = 1,0811 USD"). If your book has an exchange rate for that day, the amount is filled in from it ("your rate of 1 Oct 2026") and you can overwrite it. Where the book has none and nothing is typed, saving asks for it.
- **What stays as it is:** money moved to another of your accounts in the same transaction ("Moved to Savings: 80,00 €") and the total of an entry that came from your bank. The parts around them can be changed.
- **Messages:** "Add at least one part, so the transaction says what it was for."; "Only one part can be left without an amount; KassenSturz works that one out. Type the amounts of the others."; "The parts add up to 53,00 €, but the transaction is 53,20 €. Sort the remaining 0,20 € into a part, or leave one part's amount empty and KassenSturz works it out."; "The parts add up to 54,00 €, but the transaction is only 53,20 €. Take 0,80 € out of the parts, or leave one part's amount empty and KassenSturz works it out."; "Part 1: This part counts in USD, and your book has no exchange rate for it yet. Type what the part was in USD, e.g. from the receipt."; "Part 1: The two amounts of this part point different ways. Give both the same sign."; "Part 2: This isn't an amount. Type it like this: 1.234,56"; "This transaction can't be changed on this page. Go back to the list and open it again." (a transaction booked to the same account twice, or in another currency than the account).

## Moving money between your accounts (M3.7)

- **Type each side in its own account.** When you move 100 € from checking to savings, add "I paid money 100 €" in checking and "I received money 100 €" in savings, both with "Not sorted yet" as the category. Your bank's entries look the same, so this is also what an import adds.
- **KassenSturz asks.** After you save the second one it asks "Money moved between your accounts?": "100,00 € left Checking account on 5 Oct 2026." and "100,00 € arrived in Savings on 6 Oct 2026.", with the name each account shows. The two have to be the same amount in the same currency, at most three business days apart (Monday to Friday), in different accounts, and neither may have a category.
- **Yes, join them** makes them one move of your own money: it no longer counts as spending and income. Both accounts keep their balance and the date and name they showed, and Undo takes it apart again. **No, keep them apart** changes nothing; KassenSturz doesn't remember the answer, and asks again only if one of the two is saved again while it still has no category.
- **Several at once:** the questions come one at a time ("Question 1 of 2"). Closing the window leaves the rest apart.
- **Message:** "This choice isn't there anymore. Choose again." appears in the window if one of the two was changed while the question was open; choose "No, keep them apart" to go on.

## Undo, redo and the history (M3.8)

- **Undo and Redo** are in the side menu and on Ctrl+Z and Ctrl+Shift+Z, and they say what they act on: "Undo: Removed a transaction". With nothing to undo they are grayed out. Undo takes back the newest change first, an older one only after the newer ones, and it works after you close and open the book again.
- **A notice after every change** says what was done, with an "Undo" button: "Removed a transaction". After an undo it says "Undone: Removed a transaction" with a "Redo" button, and after a redo "Made again: …" with "Undo".
- **Telling similar changes apart:** the side menu's name is short, but Settings › Show history says what the next Undo and Redo touch: "2 transactions added · Checking account · 3 Oct 2026 to 9 Oct 2026", and for an imported statement the file's name.
- **Show history** (Settings) lists the newest 100 changes, newest first, with the day and time; the ones taken back say "taken back; Redo can make it again". A new change ends what Redo could make again.
- **Keep the history for** (Settings): only while the book is open, 1 week, 30 days (to begin with), 3 months, 1 year, or as long as the book exists. Choosing less asks first, e.g. "12 changes are older than this and will be deleted for good. You won't be able to undo them or make them again." Your accounts and transactions are not affected.
- **Clean up history** (Settings) asks first: "This deletes the whole history for good, so nothing you did so far can be undone or made again. … Copies of the book that KassenSturz saved before an update (backups) keep their history; cleaning up does not touch them."
- **Undoing a long way back:** after undoing everything, the book is empty again and KassenSturz shows the first steps; they work as on the first day.
- **Messages:** "The book doesn't match its history anymore, probably because another program changed it. Nothing was changed. To go on with Undo, open Settings and use "Clean up history"."

