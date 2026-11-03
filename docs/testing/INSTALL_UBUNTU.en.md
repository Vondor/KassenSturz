# Installing KassenSturz: Ubuntu

*Step by step; no help needed. Your password is needed once. About 15 minutes. German original (the one handed to testers): [INSTALL_UBUNTU.de.md](INSTALL_UBUNTU.de.md).*

> **Team note.** Commands match the README ("Trying a build from CI"). The release file name and screenshots from a clean Ubuntu are added once the first pre-release exists (M7a.4).

**Stuck?** Tell the facilitator. They don't install for you, but they note the spot and give you a hint.

Open a **terminal** with Ctrl+Alt+T; paste each line (Ctrl+Shift+V) and press Enter, waiting for the prompt each time.

1. **Download:** open https://github.com/Vondor/KassenSturz/releases (no account needed), under **Assets** download the file ending in `.flatpak` (not *Source code*, not the Windows file). It lands in **Downloads**.
2. **Set up Flatpak (once, needs your password):** `sudo apt install flatpak`. Nothing shows while you type the password. Answer `y` if asked.
3. **Software catalogue and KDE runtime (no password, your user only):**
   ```sh
   flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
   flatpak install --user flathub org.kde.Platform//6.11
   ```
   Confirm with `y`; the download takes a few minutes.
4. **Install:** `cd ~/Downloads` then `flatpak install --user KassenSturz.flatpak`.
5. **Start in developer mode, not from the menu:** `flatpak run --env=KASSENSTURZ_DEBUG_LOG=1 io.github.vondor.KassenSturz`. "Entwicklermodus" should show in the window title.
6. **First start:** choose "Try an example book" (German: "Ein Beispielbuch ausprobieren …", recommended: Suburban Family), otherwise create an account with an **invented** balance. Done when you see the **overview**; note the minutes (card 1).

**Update over the old version (card 6):** `flatpak install --user --reinstall KassenSturz.flatpak`. **Remove:** `flatpak uninstall --user io.github.vondor.KassenSturz`.

> **Stuck? Ask the facilitator.**
