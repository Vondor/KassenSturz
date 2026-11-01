# M6d testing guide (Phase 2)

For the person who installs the newest `main` and tries the M6d features by clicking: several dashboards, the five templates, a dashboard saved to a file and read back, the KMyMoney reports import, and *Update all accounts* with the fake bank. Everything here runs on a synthetic book. Nothing leaves the computer, no bank account and no AI service is needed, and anything can be skipped.

**Status of this document.** Written at the end of Sprint M6d Phase 1, when all steps M6d.0 to M6d.8 had merged (see `docs/ROADMAP.md`). Several designs in M6d are proposals the maintainer reacts to in Phase 2 (D-448, D-483, D-491, D-492); a design wish is not a bug and goes to the next UI round.

## Phase 1 hand-off: what Phase 2 can test without outside help

| Area | Steps | Works without outside help? |
|---|---|---|
| Several dashboards, tabs, the dashboard menu, the default dashboard | M6d.1 | Yes |
| The five templates in *New dashboard…* | M6d.2 | Yes. *Your accounts* needs the fake bank (below) to be offered |
| *Back to the template…* | M6d.5 | Yes |
| *Save to a file…* and *Add from a file…* (the round trip) | M6d.3, M6d.4 | Yes |
| The period header and filter bar as widgets | M6d.8 | Yes |
| KMyMoney reports into a new book, and into the open book | M6d.6a, M6d.6b | Yes, with the checked sample files (below) |
| *Update all accounts* | M6d.7 | Yes, with `KASSENSTURZ_FAKE_BANK=1` only |

**Where things are.** This guide: `docs/TESTING_GUIDE_M6D.md`. Sample KMyMoney files: the project files folder `test-books/kmymoney-reports/` (`kmymoney-reports-anon.kmy`, `kmymoney-reports-anon.sqlite` and a summary of what is in them). Test book for everything else: Space Colony (`crates/kassensturz-import/tests/books/space-colony-5y.json`; a copy is in the project files under `test-books/`).

**What cannot be tested yet** (named so it is not reported as a bug):
- A real bank: *Update all accounts* needs KassenSturz's own FinTS product registration (D-058) and is tried with the fake bank only.
- Windows and macOS: the Qt app on Windows waits for M7 (D-214, D-487).
- The throw-away KMyMoney book itself: the sample files stand in for step 1 and 2 of the acceptance test, so only the import side is checked.
- Anything for AI-built dashboards beyond what M6b and M6c already have; M6d adds no AI surface.

## Setup

1. Update the Flatpak to the newest `main` (the "Local repo sync" thread does this when the coordinator says "install now"). Note the build number the installer reports.
2. Start it with the fake bank:
   `flatpak run --env=KASSENSTURZ_FAKE_BANK=1 io.github.vondor.KassenSturz`
3. Open the Space Colony book with *Start a book from a file…* (Settings), so you do not use a real book. Everything in it is made up. Where a field needs a name, type a made-up word.
4. The fake bank accepts the PIN `12345` and, when it asks for one, the TAN `123456`. Both are made up and only work with the fake bank.

## A. Several dashboards (M6d.1, M6d.8)

1. Overview: with one dashboard it looks as before. Open the page menu and choose **New dashboard…**, pick *Empty*, name it. A tab row appears above the filter bar.
2. Switch with the tabs and with Ctrl+Page Down / Ctrl+Page Up. Look at the page title.
3. In the dashboard menu try **Rename…**, **Duplicate** (the copy is named *… (copy)*), **Move earlier**, **Move later**.
4. **Make this the default**: a small home icon appears on that tab. Close and reopen the book: it opens at that dashboard.
5. **Delete…** one dashboard: the question names it, and Undo (Settings › History) brings it back.
6. Edit layout on one dashboard: the period header and the filter bar are widgets. Move, resize, remove and add them again from the Add menu; at most one of each per dashboard. With the filter bar removed, the page's filters no longer apply to that dashboard.
7. Add a saved view from the Statistics page: it lands on the dashboard that is shown.

**Expect:** each action is one History entry and one Undo takes it back. No dashboard ever changes a balance or a total. Nothing overlaps.
**Report:** a lost dashboard, a wrong default after a restart, a menu entry that does the wrong thing, any changed figure.

## B. The five templates (M6d.2, M6d.5)

1. **New dashboard…**: each template is a small picture, a name and one line. Empty is first, Overview second.
2. Make one dashboard from each of: *Where the money goes*, *Money in and out*, *What you own*, *Budgets*, *Your accounts*.
3. Compare the figures on each with the Statistics page and the register for the same period. They must agree to the cent.
4. *Budgets* in a book without a budget: the dialog says so under the picture and the bars take no room. Make a budget (Space Colony has one) and look again.
5. *Your accounts* is offered only while bank connections can be used; in this setup it is. Its *Update all accounts* button is section E.
6. Change a template dashboard (move a widget, remove one), then **Back to the template…**: the question names the dashboard and how many widgets it has now and in the template; after confirming, the widgets and filters are the template's again, the name, place and default mark stay. Undo brings your version back. The entry is greyed out while the dashboard already shows its template.

**Expect:** plain words in English and German, no English text left in the German UI.
**Report:** a template whose figures differ from the Statistics page, a heading left untranslated, *Back to the template…* changing the name or the default mark.

## C. A dashboard saved to a file and read back (M6d.3, M6d.4)

1. Pick a template dashboard you changed, then **Save to a file…**. Read the note it shows about what the file holds. Save as `.json` somewhere you can find again.
2. Open the file in a text editor. It holds names of accounts, categories and groups and the formula text, and no amounts, transactions, balances, IBANs or book IDs. (Format: `docs/DASHBOARD_FILE.md`.)
3. **Add from a file…** and choose that file. A summary page comes first: how many widgets, and what in the file is not in this book (names that do not match). Nothing is added until you press **Add dashboard**.
4. Add it twice: the second copy gets a different name; neither replaces the first.
5. Try a broken file: a text file renamed to `.json`, and the saved file with a few characters deleted. Each gives a plain sentence saying what is wrong and what to do; nothing is added.
6. Start a different synthetic book (or one without a category the file names), add the file there: the unmatched names are listed and left out, and the rest of the dashboard works.

**Expect:** a file never needs your accounts' amounts; a refused file changes nothing in the book.
**Report:** an error without a next step, a file that holds money, a dashboard added without the summary, a crash on a strange file (do not send a real file).

## D. KMyMoney reports (M6d.6a, M6d.6b)

The acceptance test of `proposals/kmymoney-reports-acceptance.md` part A, from step 3, with the checked sample files. The files hold 7 reports (5 pivot tables with pie, stacked-bar and line charts, 2 detail reports) on made-up Space Colony data.

1. **Into a new book.** *Start a new book…* from `kmymoney-reports-anon.kmy`. On the done page, find the **Reports** section: every report is named, and every setting that could not be kept is listed in plain words.
2. Repeat with `kmymoney-reports-anon.sqlite`; the two summaries should agree.
3. Open the Statistics page: each imported report is a saved view. Compare picture and totals with the summary of the file (grouping, period, filters, chart type). A relative period such as *last 30 days* is shown as days beside KMyMoney's name for it.
4. Place one view on a dashboard, open another in the workbench, change a property, delete one.
5. **Into the open book.** In a book that already has data, Statistics › **Add reports from a KMyMoney file…**. The preview marks each report *Add*, *Already here*, *Keep yours* or *Replace*. By default nothing is overwritten or deleted.
6. Add them, then change one of the saved views, run the import of the same file again: the changed one is not overwritten unless you choose *Replace*; unchanged ones say *Already here*.
7. One Undo (Settings › History) takes the whole import back.

**Expect:** common reports look and add up the same; unmapped parts are reported, never silently changed; nothing is lost from the book.
**Report:** a report missing from the list, a setting dropped without a word, a wrong total, a second import that duplicates views.

## E. Update all accounts (M6d.7, human gate)

Started with `KASSENSTURZ_FAKE_BANK=1` (setup step 2).

1. Without a linked account the widget takes no room on the page. In Edit layout it shows a note: *Shows a button once an account is linked to a bank.*
2. Link two accounts to the fake bank: Accounts › **Connect a bank…** (fake bank code `37040044`, PIN `12345`, TAN `123456`), then link accounts with **Link to a bank account…**. Use a second login if you want to see two banks one after another.
3. Add the *Your accounts* template (or the widget from the Add menu, group *Bank*). Press **Update all accounts**. Nothing may start by itself on opening the book or the app.
4. While it runs the button is a progress line (*Updating 2 of 5: …*) with **Stop**. Enter the PIN and TAN when asked; cancel one once to see that bank skipped and the next one continue.
5. The summary reads like *4 accounts updated, 23 transactions booked. 2 to check.* Notices appear only for a transaction no Rule sorted or one with an unusual amount for its category; each line opens the transaction. **Seen** closes the summary.
6. Run it again at once: transactions already booked are not booked twice.
7. Settings › History: one *Imported a statement* line per account; Undo takes an account back.

**Expect:** fetched transactions are booked at once; the words never claim fraud or give advice; no payments are ever started.
**Report:** a fetch that starts without a click, a duplicate transaction, a notice that says too much, a PIN shown anywhere.

## Needs one real click

These cannot be checked by an agent in a headless run, so these are the ones that count:
- Everything above in the installed Flatpak: dragging and resizing in Edit layout, Ctrl+Page Down / Up, the tab row in a narrow window, the file chooser dialogs (Save to a file…, Add from a file…, the KMyMoney file).
- The pictures in the *New dashboard* dialog and the home icon on a tab, in both the light and the dark theme.
- German wording of every new message, dialog and error.
- The fake bank's PIN and TAN dialogs and the progress line of *Update all accounts*.
- Screen reader names (*Dashboard {name}, {n} of {count}*) and keyboard-only use.

## Short test checklist

Send with "ready to test" (`general/rules/test-checklist.md` in Agent-Teams). Name the build as the installer reports it and use Space Colony or the sample files; any item can be skipped. Left out for later rounds: a real bank, Windows, macOS.

1. A second dashboard: *New dashboard…* gives tabs, and the default survives a restart?
2. Rename, Duplicate, Move and Delete work, and one Undo takes each back?
3. Each of the five templates opens with sensible figures that match Statistics?
4. *Back to the template…* restores widgets and keeps name, place and default mark?
5. Save to a file, then Add from a file: summary first, nothing added before *Add dashboard*?
6. A broken file gives a plain sentence and changes nothing?
7. KMyMoney `.kmy` and `.sqlite` samples: all 7 reports named, unmapped settings listed?
8. Add reports into the open book twice: no duplicates, nothing overwritten without *Replace*?
9. Update all accounts with the fake bank: no start by itself, summary correct, Undo works?
10. Anything odd: a freeze, a blank screen or a wrong number, and on which screen?

**How to report.** Number and step (for example D6), what you expected and what happened. A screenshot named `YYMMDD_M6d_NN` in the screenshots folder, only of a synthetic book. Never include real keys, PINs, IBANs or real transactions. Bugs go to the "M6 issues" thread through the coordinator; layout and wording wishes are collected as one "M6d UI round".
