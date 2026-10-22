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
