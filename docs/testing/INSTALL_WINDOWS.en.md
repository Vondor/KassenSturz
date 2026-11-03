# Installing KassenSturz: Windows

*Click-by-click; no help needed and normally no administrator rights. About 10 minutes. German original (the one handed to testers): [INSTALL_WINDOWS.de.md](INSTALL_WINDOWS.de.md).*

> **Team note.** The German sheet was followed on a Windows 11 VM on 2026-11-03 with build 0.261103.18 (D-542, D-557). Screenshots: [screenshots/windows/](screenshots/windows/). Since then the installer installs for the user alone, without administrator rights, and in German (D-583). A second VM run checks it and the fixes M6-B141 to B143, B147 and B149; no Windows testers before then.

**Stuck?** Tell the facilitator. They don't install for you, but they note the spot and give you a hint so the test continues.

1. **Open the download page:** https://github.com/Vondor/KassenSturz/releases. No GitHub account needed. The top entry is newest, marked *Pre-release*.
2. **Pick the file:** under **Assets**, the one ending in **-windows-x64-setup.exe** (e.g. `KassenSturz-0.261103.18-windows-x64-setup.exe`). The **-windows-x64.zip** is the same program to unpack without installing; only if the facilitator says so. Not *Source code* and not the `.flatpak` file. About 600 MB of free space needed.
3. **Windows warnings** (the build isn't signed yet; normal for a test build, as long as the file comes from the page above):
   - Browser: click **…** next to the message, **Keep**, then **Keep anyway**.
   - SmartScreen "Windows protected your PC": click **More info** (not "Don't run"), then **Run anyway**.
4. **Install:** open the file, **Weiter** (Next), **Installieren** (Install), **Fertig stellen** (Finish). It installs into the user's own `%LOCALAPPDATA%\Programs\KassenSturz`, so no administrator rights are needed; if Windows asks anyway, cancel and tell the facilitator.
5. *Only if the facilitator tells you Windows developer mode is needed (normally it isn't):* ⚠️ it lets Windows run unchecked programs. Install only this file, then switch it off again. Settings › System › For developers (Windows 10: Update & Security › For developers).
6. **Start in KassenSturz developer mode:** Windows key, type PowerShell, Enter, then paste `$env:KASSENSTURZ_DEBUG_LOG = "1"; & "<install folder>\kassensturz.exe"` (folder added after the VM run). "Entwicklermodus" should show in the window title.
7. **First start:** page 1 (language and currency), then **Continue**; on "How would you like to start?" choose **My own accounts** for card 1 (German: "Mit meinen eigenen Konten"), or "Try an example book" ("Ein Beispielbuch ausprobieren", recommended: Suburban Family) just to look around. Create an account with an **invented** balance, then "Create my household book" ("Haushaltsbuch anlegen"). Done when you see the **overview**; note the minutes (card 1).

**Remove:** Settings › Apps › Installed apps › KassenSturz › Uninstall. It removes only the program's files; books stay (in `%LOCALAPPDATA%\io.github.vondor.KassenSturz`).

> **Stuck? Ask the facilitator.**
