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
