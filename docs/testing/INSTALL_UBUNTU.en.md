# Installing KassenSturz: Ubuntu

*Step by step; no help needed. Your password is needed once. About 10 to 15 minutes. German original (the one handed to testers): [INSTALL_UBUNTU.de.md](INSTALL_UBUNTU.de.md).*

> **Team note.** On Linux KassenSturz comes through the Snap Store (D-765); the Beta is in its *beta* channel, so way A comes first. Way B, the Flatpak file, matches the README ("Trying a build from CI"), and each release runs its steps and the removal on a fresh Ubuntu before it is made (`.github/workflows/release.yml`, [RELEASING.md](../RELEASING.md)). Screenshots from a clean Ubuntu are still to be added.

**Stuck?** Write in the [step log](INSTALL_LOG.en.md) at which step you were and what the screen showed, stop there and send the log to the facilitator. You then get a hint by e-mail.

Open a **terminal** with Ctrl+Alt+T; paste each line (Ctrl+Shift+V) and press Enter, waiting for the prompt each time.

**Two ways.** Take **way A**. Only if way A doesn't work, take **way B**.

**Way A: from the Snap Store (recommended)**

- A1. **Install (needs your password):** `sudo snap install kassensturz --beta`. Nothing shows while you type the password. At the end a line shows `kassensturz` and the version, for example `0.261117.8`. If it says "snap: command not found", take way B.
- A2. **Start in developer mode, not from the menu:** `KASSENSTURZ_DEBUG_LOG=1 kassensturz`. "Entwicklermodus" should show in the window title.

**Way B: the Flatpak file (only if way A doesn't work)**

- B1. **Download:** open https://github.com/Vondor/KassenSturz/releases (no account needed), under **Assets** download the file ending in `.flatpak`, named like `KassenSturz-0.261117.8-x86_64.flatpak` (the middle number is the version; the facilitator tells you the right one; not *Source code*, not the Windows files, not `kassensturz.snap`). It lands in **Downloads**.
- B2. **Set up Flatpak (once, needs your password):** `sudo apt install flatpak`. Nothing shows while you type the password. Answer `y` if asked.
- B3. **Software catalogue and KDE runtime (no password, your user only):**
  ```sh
  flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
  flatpak install --user flathub org.kde.Platform//6.11
  ```
  Confirm with `y`; the download takes a few minutes.
- B4. **Install:** `cd ~/Downloads` then `flatpak install --user KassenSturz-0.261117.8-x86_64.flatpak` (type `KassenSturz-` and press Tab to complete the name).
- B5. **Start in developer mode, not from the menu:** `flatpak run --env=KASSENSTURZ_DEBUG_LOG=1 io.github.vondor.KassenSturz`. "Entwicklermodus" should show in the window title.

**First start (both ways):** page 1 (language and currency), then **Continue**; on "How would you like to start?" choose "Try an example book" ("Ein Beispielbuch ausprobieren") and then Suburban Family; this is the book used in the session. Use no real numbers. Done when you see the **overview**; note the total minutes in the [step log](INSTALL_LOG.en.md).

**Remove:** way A `sudo snap remove kassensturz`; way B `flatpak uninstall --user io.github.vondor.KassenSturz`. **Warning (way A):** snap also deletes the app's data, so the test book and its backups in `~/snap/kassensturz` (snap usually keeps a snapshot, but don't rely on it). Send the log to the facilitator first; copy a book you want to keep elsewhere first. Way B keeps the test books under `~/.var/app/io.github.vondor.KassenSturz/`.

> **Stuck?** Note the step in the [step log](INSTALL_LOG.en.md) and send it to the facilitator.
