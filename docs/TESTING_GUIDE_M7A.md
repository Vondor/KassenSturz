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
2. To make a real build: tag the newest green commit on `main` as `v<number>` (`tools/build-version.sh` prints the number), push the tag, wait for the **Release** workflow, look at the draft on the Releases page, press *Publish release*.
3. A tag whose name is not the commit's number, or a tag on a commit outside `main`, must stop the first job with an error that says what to do.
4. Download the file from the published release without being logged in to GitHub (a private window), and install it with the Ubuntu sheet.
