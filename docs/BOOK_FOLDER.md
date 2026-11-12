# Choosing the folder for the book

**Status:** design for US5.3 (#515), decisions D-730 to D-733 (Proposed) and question Q-510 in `docs/DECISIONS.md`. Nothing here is built yet; the build is slotted before Beta (US6). Stage: Alpha, so no migration is involved (D-274): the book's own layout doesn't change at all.

## 1. Today

- Outside the Flatpak the user can open a book anywhere, start a new one with a save dialog and start a book from a file with a save dialog (D-113, D-125, Q-081 answered B).
- In the Flatpak the book always lives in KassenSturz's own data folder. The Flatpak has no access to the user's files (`packaging/flatpak/io.github.vondor.KassenSturz.yml`, "Deliberately nothing else"), and the file chooser portal hands over one file at a time. A book needs two neighbours it creates itself: its lock file `<book>.lock` (D-038, D-080) and SQLite's rollback journal `<book>-journal` (D-033). So a single handed-over file is refused as `book.sandboxed` (`platform::Drive::Sandboxed`, file system `fuse.portal`), and `platform::can_choose_book_place` returns false in the Flatpak.
- The "Folder" row in Settings only opens the data folder.

The wish (#515): the user chooses the folder their book lives in, also in the Flatpak, e.g. a sync folder or a folder on a second drive.

## 2. What the user sees

One row in **Settings › Your data**: **"Where your book is kept"**, showing the folder in the user's own words (the real path on their computer, never a sandbox path) and a button **"Keep it in another folder…"**.

1. The system's folder chooser opens. The user picks a folder.
2. A page shows: the folder, the book's name (editable, `.ksz` added), its size, and one plain sentence: "KassenSturz moves your book there. Before that, it keeps a copy of your book as it is now with its backups on this computer, so you can always go back."
3. **"Move my book"**. A short progress line, then a done page: where the book is now, where the copy before moving is, and **"Move it back"**.

"Start a new book…" and "Start a book from a file…" ask for the folder the same way in the Flatpak (folder chooser, then the name on the app's own page). "Open another book…" in the Flatpak asks for the folder and then lists the books (`.ksz` files) in it; if there is exactly one, it opens at once.

Everything else stays as it is. Someone who never touches this row keeps their book in KassenSturz's own folder, exactly as today (progressive disclosure, AGENTS.md §6).

## 3. D-730: the folder portal, not a filesystem permission

### Options

| | A) Folder through the file chooser portal **(recommended)** | B) `--filesystem=home` (Q-071 A) | C) `--filesystem=xdg-documents` (Q-071 B) | D) No change (Q-071 C) |
|---|---|---|---|---|
| What the app may reach | only the folders the user picked, each by a deliberate choice | the whole home folder, at all times | the whole Documents folder | only its own data folder |
| Sync folders, second drives | yes, any folder the user picks | only those under home | only under Documents | no |
| New Flatpak permission | none: the portal and the document portal's mount are there in every sandbox | yes, a broad one; Flathub asks for a reason | yes | none |
| With `--share=network` (D-142) | a compromised app reaches only the picked folders | a compromised app could read every file in home and has network | Documents and network | — |
| Usable for the bill folder (US5.13, #717 parts 5 and 6) | yes, the same mechanism | yes | only under Documents | no |
| Works outside the Flatpak | the same code: Qt's folder dialog returns a normal path | — | — | — |

Q-071 was written when the app had no network access; since D-142 the Flatpak has `--share=network` for bank connections. A broad file permission plus network is exactly the pairing a sandbox is meant to avoid (AGENTS.md §2 principle 2). A keeps the sandbox as narrow as today and adds only what the user picks.

### How it works

- **Choosing:** Qt's folder dialog (`QFileDialog::getExistingDirectory`, through `FolderDialog` in QML). In the Flatpak, Qt uses the file chooser portal (`org.freedesktop.portal.FileChooser.OpenFile` with the option `directory`, portal interface version 3 or later). The portal exports the folder through the document portal; the app sees it at `$XDG_RUNTIME_DIR/doc/<id>/<folder name>/` and may create, change, rename and remove files **inside** it. That is what the lock file and the journal need.
- **Keeping the grant:** the document portal remembers an export made through the file chooser across restarts, under the same `<id>`. The app remembers the book's path in `kassensturzrc` with `$XDG_RUNTIME_DIR` written as such (like `$HOME` today, D-113), so it doesn't depend on the user's number on that computer.
- **The real path, for showing only:** the document portal's `GetHostPaths` (interface version 5 or later, over D-Bus) gives the folder's path on the computer. The app stores it next to the book's path, as **display text only**: it is shown on screens and used for the network check in D-732, never opened. If the portal is too old for it, the app shows the folder's name and says "a folder you chose".
- **A shared helper** in `crates/kassensturz-ui-bridge/src/platform.rs` (later `kassensturz-platform`): `choose_folder` returns a `FolderGrant { path, shown_as }`, and `can_choose_book_place` becomes "outside the Flatpak, or a portal with folder support". US5.13's bill folder uses the same helper, so this design keeps that path open.
- **`Drive::Sandboxed` narrows:** it now means only a single file the portal handed over, i.e. a file whose parent is the `<id>` folder itself. A file inside an exported folder (`<id>/<folder>/<file>`) is a normal book.
- **When there is no folder portal** (a very old desktop): the row says plainly that this computer's system doesn't offer it and the book stays in KassenSturz's folder, which keeps working.

### What the build checks first, in a real Flatpak (step 0)

This session could not reach the portal's online documentation, so the facts above come from the portal's specification as known before; the build confirms each one before it relies on it, and records the result in D-730:

1. The folder dialog in the Flatpak returns a `/run/user/<n>/doc/<id>/<folder>` path, on Plasma and on GNOME.
2. In that folder, SQLite opens, writes and commits a book with the rollback journal; the lock file is created and removed; a rename inside the folder works; `flock`/POSIX locks behave (SQLite locks only within one computer anyway, D-038).
3. The grant and the `<id>` survive closing the app, logging out and a restart of the computer.
4. Taking the grant back (`flatpak permission-reset io.github.vondor.KassenSturz`, or a permissions tool) leads to `book.no-access` (D-732), never to a new book.
5. `GetHostPaths` returns the real path.

If 2 fails, the design falls back to asking Vondor about B or C, with the failing check named.

## 4. D-731: moving a book, with a backup and a way back

**Only moving is offered, not copying into a second live book.** Two working books of the same household drift apart, and an everyday user can't tell which one is real. A copy for keeping already exists: "Save a copy of my data › The whole book" (D-125).

### The steps

Before anything is written, the page checks and refuses with a plain reason (each its own code, D-034):

- the folder is the one the book is in already (`book-move.same-folder`);
- a file `<name>.ksz` or `<name>.ksz.lock` is already there (`book-move.already-exists`; nothing is ever replaced, D-113);
- the folder can't be written to: the app creates and removes `<name>.ksz.part` (`book-move.read-only`);
- the free space, where the system tells it, is less than twice the book's size (`book-move.no-space`); where it doesn't tell, no claim is made and step 3's failure covers it.

Then, while the screen allows no other change:

1. **Backup.** The book as it is goes into KassenSturz's backup folder through the existing backup code (D-033, D-080), named "… before moving, <date>". These backups are kept apart from the ten automatic ones: the last three moves are kept.
2. **Copy.** SQLite's online backup copies the open book to `<name>.ksz.part` in the new folder (a name sync tools such as Nextcloud leave out by default, so a half copy never travels), then `fsync`.
3. **Check.** The copy is opened read-only and must have KassenSturz's application ID (D-080), pass `PRAGMA integrity_check`, have the same layout version and the same number of rows in every table, and give the same balance for every account as the open book. Balances are computed by code (AGENTS.md §2.1).
4. **Into place.** `<name>.ksz.part` is renamed to `<name>.ksz`, after checking once more that no such file appeared meanwhile.
5. **Switch.** The new book is opened with its own lock and remembered as the book opened last; only then is the old book closed and its lock removed.
6. **Old file.** The old file is removed. The backup from step 1 is the identical copy, so nothing is lost. If removing fails (e.g. a read-only folder), it stays and the done page says where.

**If any step fails**, the `.part` file is removed where possible, the old book stays open and stays the book opened last, and the page says what failed and what to do next (AGENTS.md §6 "No dead ends"). Nothing has changed for the user.

### The way back

- **"Move it back"** on the done page and in the "Where your book is kept" row ("Moved on <date> from <place>") runs the same steps back to the old folder. KassenSturz's own folder is always offered as "KassenSturz's own folder" without the folder chooser.
- The backup from step 1 is listed with the other backups; opening it is restoring it (D-037).
- Moving is not on the Undo list, because it doesn't change the book's content; the done page says so in one sentence, with the two ways back.

## 5. D-732: a folder that is missing, not allowed, read-only, or on a network drive

**Backups always stay on this computer**, in KassenSturz's own data folder (D-033), wherever the book is. So a lost drive or share never takes the last way back with it.

| Situation | What the app does | Code |
|---|---|---|
| The book isn't there: drive not connected, folder renamed or deleted, share offline | As today: never a new book in its place; says where it was (the real path), offers "Try again", "Open another book…", "Start a new book…" and the backups | `book.missing` (D-113) |
| The Flatpak may no longer reach the folder (grant taken back, portal data reset) | Says so plainly and offers **"Choose the folder again…"**, which opens the folder chooser at the last real path. If the chosen folder holds a file with the remembered name and KassenSturz's application ID, it opens. Otherwise the same choices as `book.missing` | `book.no-access` (new) |
| The folder or file is read-only when opening (the lock file can't be made: `EROFS`, `EACCES`) | Says the book can't be changed there and offers **"Keep it in another folder…"** (D-731; step 6 then leaves the old file and says so) and the usual choices. No read-only viewing mode in this build | `book.read-only` (new; the store already names `read-only`) |
| Writing fails while the book is open (share drops, stick pulled, disk full) | The change in progress isn't saved: SQLite's journal rolls it back, so the book is as it was before that change. The app stops writing, shows a page that says exactly that, and offers "Try again" (reopen) and the backups. No silent retry loop | `book.write-failed` (new) |
| A network drive outside the Flatpak | Unchanged: opens with the warning on every screen (D-038), detected from the file system type | — |
| A network drive inside the Flatpak | The sandbox sees only `fuse.portal`, so the file system type can't tell. The app checks the real path from `GetHostPaths` against the mount points of GVfs (`$XDG_RUNTIME_DIR/gvfs/`) and KIO FUSE (`$XDG_RUNTIME_DIR/kio-fuse-*`); a match gets D-038's warning. Otherwise no claim is made, and the move page says once, in plain words, that a book on a network drive or in a sync folder must be closed on the other computer first (the lock file shows this, D-038) | — |
| A sync folder (Nextcloud, Dropbox, Syncthing) | Unchanged: the lock file and the rollback journal (D-033, D-038) | — |

## 6. D-733: what changes in earlier decisions

- **D-113:** "Open another book…" in the Flatpak chooses a folder and then a book in it. `book.sandboxed` remains only for a single file the portal handed over. A remembered book path may be a document-portal path, kept with `$XDG_RUNTIME_DIR`, plus its real path as display text. New codes `book.no-access`, `book.read-only`, `book.write-failed`. The rest stands (a missing book is never replaced, an existing file never replaced, a book that doesn't open leaves the open one open).
- **D-125:** "where the platform allows it" now includes the Flatpak whenever the portal offers folders: there the user picks the folder and types the name on the app's page, because a single file from the save dialog can't hold a book. Outside the Flatpak the save dialog stays. The empty-file case of saving a copy is unchanged.
- **Q-071:** answered by D-730 with a fourth option, "the folder portal, no file permission", once Vondor answers Q-510. Its note "the app has no network access" is out of date since D-142.
- **Q-081:** answer B now reaches the Flatpak too; the data folder stays the default for anyone who doesn't choose.
- **D-033:** backups stay in the data folder; the three "before moving" backups are kept apart from the ten automatic ones.
- **D-038:** unchanged, plus the Flatpak's limited network detection above.
- **The Flatpak's permission list** and CI's comparison with it don't change.

## 7. Build in US6 (before Beta)

Three small pull requests, standard high, each with its German strings (`tools/update-translations.sh`, `tools/check-translations.sh`):

1. **Folder grant** (shared with US5.13): step 0's checks recorded in D-730; `choose_folder`, `FolderGrant`, the narrower `Drive::Sandboxed`, `can_choose_book_place`, paths kept with `$XDG_RUNTIME_DIR`. Tests: path classification (single file, file in an exported folder, normal path), the remembered path round trip.
2. **Moving** (D-731): the page, the checks, steps 1 to 6, "Move it back". Tests with a real book in temporary folders: success, each refusal, a failure injected at each step leaves the old book open and no `.part`, the copy check catches a changed row and a changed balance, the backups kept apart from the ten.
3. **Problems and the Flatpak paths** (D-732, D-733): the three new codes and their pages, "Open another book…", "Start a new book…" and "Start a book from a file…" through the folder in the Flatpak. Bridge objects read back the way QML does (AGENTS.md §9.4).

Phase 3 then checks it by hand in the Flatpak on Plasma and GNOME: move to a folder, restart, take the grant back, pull a stick while the book is open.

**Human gates:** none expected: no new dependency (Qt's folder dialog and Qt's D-Bus module come with Qt in the KDE runtime; if the build finds it must add a CMake component, it asks first), no Flatpak permission, no keychain, bank or AI change, no change to the book's layout.
