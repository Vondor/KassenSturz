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

## M7a.4 A downloadable build

For the maintainer; the steps and the reasons are in `docs/RELEASING.md` (D-579).

1. Open a pull request that changes `.github/workflows/release.yml` (or look at the one that added it): the **Release** workflow runs its build and its install check without making a release. The install check's step *Start the app* shows the app's output and the debug log's folder.
2. To make a real build: tag the newest green commit on `main` as `v<number>` (`tools/build-version.sh` prints the number), push the tag, wait for the **Release** workflow; the pre-release is on the Releases page when it ends.
3. A tag whose name is not the commit's number, or a tag on a commit outside `main`, must stop the first job with an error that says what to do.
4. Download the file from the published release without being logged in to GitHub (a private window), and install it with the Ubuntu sheet.

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

**Language on page 1** (Q-395). Choose *English* on the German welcome page: a line says *KassenSturz startet neu auf English. Es geht nichts verloren.* Press **Continue**: KassenSturz closes and starts again by itself, in English, on the welcome page. *Settings* shows the same choice. In the Flatpak the new KassenSturz starts in a sandbox of its own through `flatpak-spawn`, with the `KASSENSTURZ_*` variables it was started with; check there that it comes back, and in test mode that the banner is still there.

**Page 4, how new entries get in.** After **Create my household book** the overview opens with *How do new entries get in?* on top. **I'll type them myself** goes back to the overview. **Import a file from my bank** asks *Which account is the file for?* (only with more than one account), then opens the usual file dialog and import pages; try a synthetic CAMT or CSV file from the tester kit. Going back from page 4 is the overview. **Connect to my bank** is not there in normal builds (Q-398); only a build with `-DKASSENSTURZ_WIZARD_BANK=ON` (cargo feature `wizard-bank`) shows it, and only where bank connections work.

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

**Bank in test mode** (D-547).

7. Start with `KASSENSTURZ_TEST=1` only (no fake bank variable). *Bank connections* show the made-up bank, marked as such, and no real bank can be set up.
8. In *Settings*, choose KeePassXC as the place for saved PINs, then save a PIN for the made-up bank. KeePassXC is not asked and gets no entry; after a restart in test mode the PIN is asked again.
9. With `KASSENSTURZ_FAKE_BANK=keychain` the PIN goes to the system keychain under `io.github.vondor.KassenSturz.FakeBank`, as before (D-159): that variable asks for the keychain by name.

## M7a.10 Adjustable chart colors

**What it is.** The eight chart colors can be changed in *Settings*, for this computer only (like how amounts look): they are not saved in a book, so every book's charts use them (D-589). The standard colors are the colorblind-safe set of D-283. A power user can also name colors in a custom chart's description.

**Click path (Settings).**
1. Open *Settings* and scroll to *Chart colors*. There are eight rows: a swatch, the number and name (1. Blue … 8. Grey), a field with the color, **Choose…** and **Standard**. **Back to standard for all** is greyed out.
2. In row 1, select the field, type `#1a2b3c` and press Enter. The swatch changes and the field shows `#1A2B3C`. The row is now called *1. Own color* instead of *1. Blue*, because the name would say the wrong color; **Standard** brings the old name back. **Standard** in that row and **Back to standard for all** can now be pressed.
3. In row 2 type `0, 114, 178` and press Enter: it becomes `#0072B2`.
4. In row 3 type `nonsense`: the row says *That is not a color…* and nothing changes. Type `256, 0, 0` in row 4: it says red, green and blue each go from 0 to 255. Type a good color: the sentence goes away.
5. In row 5 press **Choose…**. The system's own color dialog opens (on Plasma, the one every KDE program uses). Pick a color and press OK: the swatch and field change. Clicking the swatch itself opens the same dialog.
6. Open the *Overview* or a *Statistics* chart: the first series uses color 1, the second color 2, and so on. The new colors are used at once; there is no restart.
7. In row 6 enter a color almost like row 5's: the row says *This color looks very much like the next one…*, and row 5 says it looks like the one before. Enter near-white in any row on a light theme: it says *hard to see against the background*. A warning never stops you; keep the color if you want.
8. **Standard** puts one color back, **Back to standard for all** all of them. Close and start KassenSturz: your colors are still there.
9. **Add a color** (below the list) adds a ninth row, *9. Wine red*, with its own standard color; you can go up to 200 (no chart has more than 200 colored parts; the page takes a second or two to rebuild with that many rows). **Remove** in a row takes that color out and the ones below move up (a row that now holds another place's color is called *Own color*); you can go down to 2, and **Remove** is then greyed out. Close and start KassenSturz: the number of colors and your colors are still there. **Back to standard for all** brings back the eight standard colors. A chart with more parts than colors starts again at color 1. At 2 colors **Remove** is greyed out; at 200 **Add a color** is.

**Click path (Report designer).** Open *Statistics*, choose a *Custom chart* and select it. Under the description field, *Colors you can name in a description* shows the eight places with their words (blue, vermillion, …) as they are set now. There is no color field; **Edit palette…** opens *Settings*.

**For power users, in the description.** `"config": {"range": {"category": ["#cc3300", "sky-blue"]}}` colors the first series with exactly #CC3300 and the second with the palette's sky blue; `"mark": {"type": "bar", "color": "#336699"}` colors a layer that has no `color` channel. Anything else (`#fff`, `rgb(…)`, a name such as `red`) is listed under *Not supported here* with its place, and the rest of the chart is still drawn if you choose that.

**Expected.** Colors are text everywhere between Rust and the screen (`#RRGGBB`); no color is a number in QML. A hand-edited settings file with a bad color shows the standard color for that place. Charts in the AI chat use the same palette.

**Real-app screenshots** (Xvfb): see the pull request for M7a.10.
