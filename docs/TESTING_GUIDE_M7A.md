# M7a testing guide

Click paths for the M7a steps, added by each step as it merges. Everything here runs on synthetic books only (D-552). This file is for the people who build and check the program; the guides for testers are in `docs/testing/`.

## M7a.2 Try an example book

**What it is.** Example books are made-up households that come with the app. Picking one always makes a new book called *Example – <name>* and opens it; a book you already have is never changed (D-570, D-571). The four examples are *Suburban Family* (the suggested start), *Flower Utopia*, *Space Colony* and *USSR Government Agent*. *Flower Utopia* also comes with its five budgets and a saved Overview with charts (M6-B138); the other three are plain books.

**Click path.**
1. Open *Settings* (the gear in the side bar).
2. Under the book entries press **Try an example book…**. The page *Try an example book* opens, with one row for each example and its one-line description.
3. Press **Space Colony**. KassenSturz makes the book *Example – Space Colony* in its data folder, opens it, and shows the overview.
4. Check: *Settings › Open another book…* shows the book you had before; it is unchanged. Pressing the same example again makes *Example – Space Colony (2)*.

**Expected.**
- The overview shows the colony's accounts and spending; the register of any account lists thousands of transactions.
- The bundled originals are never opened or changed.
- All texts are in the UI language (German with the German UI). The German names of the books may change when M7a.8 settles them.

**Real-app screenshots** (Xvfb, a fresh settings folder): see the pull request for M7a.2.

**Not in this step.** The first-start entry (the wizard owns it, M7a.9), the three other example books (M7a.8), the matching bank statement files (M7a.3, M7a.8).

## M7a.9 First setup wizard, pages 1 to 3

**What it is.** A book without accounts opens the setup wizard (`docs/SETUP_WIZARD.md`) in place of the old first-run pages. Nothing is written until **Create my household book**, which writes the currency, the default categories and all accounts in one step. One undo takes the whole setup back, and the wizard shows again.

**Click path** (a fresh settings folder, so the app starts with its own empty book):
1. Page *Welcome*: check that the currency is preselected (the regional one, otherwise EUR), then press **Continue**.
2. Page *How would you like to start?*: three choices of equal size, *My own accounts* first, plus the small link *Open a KassenSturz book I already have…*. Press **My own accounts**.
3. Page *Your accounts*: one row, *Checking account*. Type `2.340,17` in the German UI (`2,340.17` in English). Press **Add another account** three times: the rows come as *Savings account*, *Cash*, *Credit card*. Give them `8.500,00`, nothing, and `412,30`; the credit card row says *You owe:*.
4. In the German UI, type `12.50` into one amount and leave the field (a point groups thousands there): the row shows that the amount isn't one. Correct it.
5. Press **Create my household book**. The overview opens with the message *Your household book is ready…* and the four accounts; together they come to 10.427,87 €.
6. Press **Got it**: the message goes. Press Ctrl+Z: the whole setup is taken back, and the wizard shows again.

**Also check.**
- Close the window on page 3 after typing: at the next start the wizard shows again, and the book has no categories.
- Page 2 › **Try an example book** opens the M7a.2 chooser; page 2 › **Bring my data from another program** opens the existing *Start a book from a file* dialog.
- *Add an account* from the overview no longer says *Your first account*, and adding a transaction has no *Skip for now*.

**Not in this step yet.** The language choice on page 1 and page 4 (bank file and bank).

## M7a.9 Test mode kept apart from the real setup

**What it is.** Started with `KASSENSTURZ_TEST=1` or with the fake bank (`KASSENSTURZ_FAKE_BANK`), KassenSturz keeps every book, backup, setting and log in a test folder beside its real data (on Linux `~/.local/share/kassensturz-test`, in the Flatpak inside the app's own data folder) and never opens or reads a book outside it (Q-399, `docs/SETUP_WIZARD.md`). `KASSENSTURZ_DEBUG_LOG=1` alone doesn't start it.

**Click path** (on a computer that has a real book):
1. Start KassenSturz with `KASSENSTURZ_TEST=1`. Every screen shows the blue banner *Test mode: … in <folder>*; **Show the folder** opens it.
2. The first start in the test folder shows the wizard. **Continue** goes straight to *Try an example book*. Pick one: it opens, and the next start in test mode opens it again.
3. *Settings › Open another book…*: choose the real book. KassenSturz refuses with *The test mode only opens books in its own folder*, and the real book gets no `.lock` file.
4. *Settings › Start a book from a file…*: choose a KMyMoney file outside the test folder. The message says the file wasn't read.
5. *Settings*: there is no AI switch.
6. Close and start KassenSturz the usual way: the real book opens as before, with its own settings and language.

**Starting fresh.** Delete the test folder.

**Not in this step yet.** Saved PINs and real bank logins in test mode (bank connections are a human gate, so they come in a pull request of their own).
