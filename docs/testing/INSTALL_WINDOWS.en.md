# Installing KassenSturz: Windows

*Team copy of [INSTALL_WINDOWS.de.md](INSTALL_WINDOWS.de.md), the sheet handed to testers. Click-by-click; no help needed and normally no administrator rights. About 10 minutes.*

> **Team note.** Windows testers install from the Microsoft Store (D-854); the Store route was checked in the Windows 11 VM under #513. Way B (the installer file from the download page) is only for when the Store is blocked; the tester notes why in the step log. Screenshots: [screenshots/windows/](screenshots/windows/) (the Store pages follow). The sections "Find the log" (a stopgap for B-381, #1177) and the uninstall warning (B-382, #1178) stay until D-856 ships (PR #1180). Then "Remove" says that the book and backups stay in `Documents\KassenSturz`.

**Stuck?** Write into the [step log](INSTALL_LOG.en.md) which step you were at and what the screen said, stop there and send the log to the facilitator. They reply with one hint by e-mail.

**You need:** Windows 11 (or 10) with a normal user account; an internet connection for the download; for way A access to the Microsoft Store (a Microsoft account is usually not needed); about 600 MB free.

## Way A: from the Microsoft Store (recommended)

- **A1. Open KassenSturz in the Store:** in the browser open **https://apps.microsoft.com/detail/9NM5V48PPRLQ** (the page is called "KassenSturz - The Household Budget App"). Use this link; a web search for the name shows something else first. If the browser asks whether to open **Microsoft Store**, choose **Open**.
- **A2. Install:** click **Install** (sometimes **Get**), wait. No administrator rights, no security warning. **KassenSturz** is then in the Start menu.
- **A3. Developer mode:** Windows key, type PowerShell, Enter, then paste `[Environment]::SetEnvironmentVariable("KASSENSTURZ_DEBUG_LOG", "1", "User")`. Close PowerShell and start KassenSturz from the **Start menu**. "KassenSturz – Entwicklermodus" should show at the top of the window with a red banner. After the test switch it off with the same line with `$null` instead of `"1"`.

## Way B: installer file (only if way A fails)

- **B1.** Open https://github.com/Vondor/KassenSturz/releases (no account needed); the top entry is the newest Beta, marked *Pre-release* (or *Latest* when it is also the stable release).
- **B2.** Under **Assets** take the file ending in **-windows-x64-setup.exe**, for example `KassenSturz-0.261117.8-windows-x64-setup.exe`. Not the `.msix` (for the Store), not the `.zip` unless told, not *Source code*, not `.flatpak`, not `kassensturz.snap`.
- **B3. Warnings** (the installer is not signed; normal for a Beta if the file comes from that page): in the browser **…**, **Keep**, **Keep anyway**; at SmartScreen "Windows protected your PC" click **More info** (not "Don't run"), then **Run anyway**. The Store route (A) has no such warning.
- **B4. Install:** open the file, **Weiter**, **Installieren**, **Fertig stellen**. It installs into your own folder `%LOCALAPPDATA%\Programs\KassenSturz`, so no administrator rights; if Windows asks anyway, cancel and note it in the log.
- **B5. Developer mode:** in PowerShell run `$env:KASSENSTURZ_DEBUG_LOG = "1"; & "$env:LOCALAPPDATA\Programs\KassenSturz\bin\kassensturz.exe"`. **The very first start can take up to 7 minutes** (Windows checks every file once); don't start it again meanwhile. Keep the PowerShell window open until you are done.

## First start (both ways)

Page 1 (language and currency), then **Continue**; on "How would you like to start?" choose "Try an example book" ("Ein Beispielbuch ausprobieren") and then Suburban Family; this is the book used in the session. Done when you see the **overview**; note the total minutes in the log.

## Find the log (for your report)

For the Store version the **Protokoll zeigen** button does not yet open the right folder (B-381). Windows key + R, then paste `%LOCALAPPDATA%\Packages\47425SerpentSoftware.KassenSturz-TheHouseholdBudge_6gxp6ch7ybyne\LocalCache\Local\io.github.vondor.KassenSturz\debug-log`; the file **current.jsonl** is the log; send it with your report. With way B the log is in `%LOCALAPPDATA%\io.github.vondor.KassenSturz\debug-log`.

## Remove

- **Way A (Store):** Start menu, type "Kassen", **KassenSturz** › Options (or right-click) › **Uninstall**. **Warning:** Windows then deletes the app's whole data folder, with the book in its default place, all backups and the log. **Send the log to the facilitator first.** Harmless for the test (example book).
- **Way B:** Settings › Apps › Installed apps › KassenSturz › Uninstall. Your test books stay as files until you delete them (in `%LOCALAPPDATA%\io.github.vondor.KassenSturz`).
