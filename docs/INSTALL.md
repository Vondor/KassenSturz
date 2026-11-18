<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Installing KassenSturz

*German: [INSTALL.de.md](INSTALL.de.md).* KassenSturz is in Beta. Try it with made-up numbers or an example book, never with your own data. Step-by-step sheets for test sessions are in [`testing/`](testing/README.md).

## Linux

There are two ways. Both install the same app; take one, not both.

### A. From the Snap Store (recommended)

```sh
sudo snap install kassensturz --beta
```

Then start **KassenSturz** from the app menu. Updates come by themselves, as for every snap.

**Before you remove the snap, move your books out.** `sudo snap remove kassensturz` deletes the snap's own folder `~/snap/kassensturz/`, and a book made at the first start lies there, with its backups. snapd keeps a copy for 31 days (`snap saved` lists it, `sudo snap restore <number>` brings it back after you install the snap again), but don't rely on it: copy the books you want to keep somewhere else first, or use "Save the whole book" in the app. A book you keep in a folder of your own, such as Documents, is not touched.

The Beta is in the store's *beta* channel; there is no *stable* release before 1.0. A few things are switched off until you allow them, because they reach other parts of your computer:

- saving the banking PIN in the keychain: `sudo snap connect kassensturz:password-manager-service` (without it, the app asks for the PIN each time);
- a chip-card reader for bank logins: `sudo snap connect kassensturz:pcscd`;
- books on USB drives: `sudo snap connect kassensturz:removable-media`.

You can see and change these switches with `snap connections kassensturz`, or in KDE Discover and Ubuntu's Settings under the app's permissions. Details: [`SNAP.md`](SNAP.md).

### B. The Flatpak file (direct download)

On the [releases page](https://github.com/Vondor/KassenSturz/releases), the newest release has a file ending in `.flatpak` under **Assets**. Once per computer, add the catalogue that has the KDE runtime, then install the file:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.kde.Platform//6.11
flatpak install --user KassenSturz-<version>-x86_64.flatpak
```

KassenSturz itself is not on Flathub (D-765); the catalogue is only where the KDE runtime comes from. A newer file is installed over the old one the same way. To remove it: `flatpak uninstall --user io.github.vondor.KassenSturz`.

Removing the Flatpak keeps its folder `~/.var/app/io.github.vondor.KassenSturz/`, with the books and backups in it, unless you add `--delete-data`.

## Windows

### A. From the Microsoft Store (recommended)

Install KassenSturz from the [Microsoft Store](https://apps.microsoft.com/detail/9NM5V48PPRLQ). The Store signs the program and installs updates by itself, so Windows shows no warning.

### B. The installer from the releases page

On the [releases page](https://github.com/Vondor/KassenSturz/releases), download the installer of the newest release and run it. It is not signed, so Windows shows a SmartScreen warning first (D-542, [`CODE_SIGNING_POLICY.md`](CODE_SIGNING_POLICY.md)). Details on the Windows packages: [`WINDOWS.md`](WINDOWS.md).

## macOS

Not yet. A signed `.dmg` follows once its signing is funded (D-048).
