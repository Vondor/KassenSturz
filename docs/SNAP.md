# KassenSturz as a snap

**Status:** built for #892 (US7, D-765). `packaging/snap/snapcraft.yaml` and `.github/workflows/snap.yml` are in; **the snap has not run on a real desktop yet**, only in CI's install check. Publishing is the maintainer's step (AGENTS.md §3.2). All names here are invented or the project's own.

## 1. What a user gets

The Snap Store (D-765) reaches Ubuntu and other distributions; the own Flatpak stays as a direct download and there is no Flathub submission. The snap is `strict`. Like the Flatpak it works through the desktop portals; unlike the Flatpak, a snap's permissions are the interfaces `home` and `removable-media`, which the user can switch off (and, for removable media, on) at any time.

## 2. How it is built

- Base `core24`; Qt 6 and KDE Frameworks come from the `kf6-core24` content snap through the `kde-neon-6` extension. The extension adds the plugs the window needs (desktop, wayland, x11, opengl, network).
- The bank libraries are built as parts from the same pinned tarballs and options as the Flatpak (D-054, D-142, `docs/THIRD_PARTY.md`): pcsc-lite client, Gwenhywfar with Qt 6 dialogs, libchipcard, AqBanking with only the FinTS backend, no command-line tools (D-053). They are found by their ABI names (D-138). Their compiled-in `/usr/...` folders are mapped by `layout`.
- The `kassensturz` part runs the project's CMake, which drives Cargo (D-063). Unlike the Flatpak it builds online (Cargo downloads crates from `Cargo.lock`, `--locked`); the Rust comes from rustup because the workspace asks for a newer one than the build image has.
- The build number comes from `tools/build-version.sh` (D-152, D-226), written to `build-version` by the workflow. `grade` is `stable` only for an official tag, else `devel`.
- The workflow builds on an `ubuntu-24.04` runner with `--destructive-mode` (the runner is the base), installs the file with `--dangerous`, starts the app under a virtual display, and removes it. It runs on pull requests that touch `packaging/snap/` and for every tag, from `release.yml`.

## 3. The registration number (D-745)

For a tag, `snap.yml` writes the secret `FINTS_REGISTRATION` of the environment `release` to the git-ignored file `fints-registration`, as the Flatpak job does. A step then unpacks the snap and fails unless the number is in `usr/bin/kassensturz` and nowhere else. Pull requests build with a made-up number of the right shape, so the search is proven on every change.

## 4. Confinement: what the user sees

| Need | Interface | Auto-connected? | What the user does |
|---|---|---|---|
| Window, GPU, network, portals | `desktop`, `wayland`, `x11`, `opengl`, `network` (from the extension) | yes | nothing |
| Saved banking PIN in the keychain (Secret Service, KWallet) | `password-manager-service` | **no**: the store's reviewers rarely grant auto-connection, because the interface reads every stored secret | `sudo snap connect kassensturz:password-manager-service`, or use the PIN each time. The app already works without a keychain (D-057) |
| Chip-card login | `pcscd` | **no** | `sudo snap connect kassensturz:pcscd` |
| Your home folder (books, bill and backup folders directly, Q-570) | `home` | **yes** on desktops (store snaps) | nothing; to take it away: `sudo snap disconnect kassensturz:home` |
| USB drives and other removable media (books and copies there) | `removable-media` | **no** | `sudo snap connect kassensturz:removable-media`; take away with `snap disconnect` |
| Folders the user picks with the folder chooser (D-730, D-739, D-763) | portals, as in the Flatpak. The app treats a snap as a sandbox like the Flatpak (`platform::in_sandbox`): a new book is chosen by folder, and the portal's mount is found above the snap's own `$XDG_RUNTIME_DIR` (`.../snap.kassensturz`). For a book the app asks the portal for the folder's real path; if the home folder or removable media is allowed and the real path is reachable, **the book is opened there directly**, with working file locks. If not, the portal's path is used only where the system lets the app lock files (it does not in a snap, B-265), and otherwise the chooser says plainly that the app is not allowed to keep a book there and what to allow. | n/a | see "Seeing and changing what the snap may do" |
| KeePassXC as the keychain (D-164) | would need access to another app's socket folder | no | **not available in the snap**: the Flatpak's `xdg-run/app/org.keepassxc.KeePassXC` permission has no strict-snap counterpart without `system-files`/`personal-files`, which the store reviews by hand. Listed as a known limit. |

Requesting auto-connection for `password-manager-service` and `pcscd` is the maintainer's choice (a forum request to the store, see section 8); the default is no request, which keeps the safest behaviour (the user decides who may read secrets).

### Seeing and changing what the snap may do

Flatseal's role for the Flatpak is played by snapd itself:

- **Command line:** `snap connections kassensturz` lists every interface and whether it is connected; `sudo snap connect kassensturz:removable-media` and `sudo snap disconnect kassensturz:home` change it. The change applies at the next start of the app.
- **Graphical:** KDE Discover (the app's page, *Permissions*), Ubuntu's *Settings > Applications*, and the Snap Store app show the same switches.
- **What happens without them:** with `home` disconnected, books live in the app's own folder or in folders the portal exports; the folder chooser refuses a folder where locks cannot work, with the message "KassenSturz is not allowed to keep a book in this folder. Allow it to use your home folder or removable media in your system's app permissions, then choose the folder again." Nothing is lost: a book already open stays where it is; if its folder is no longer reachable the app says so on the next start (D-730's "choose the folder again" page).

**AI off (D-013, D-050):** unchanged by packaging. The MCP server is the second snap app `kassensturz.mcp`, stdio only, and refuses to start unless AI is on; the snap adds no network listener. Build option `KASSENSTURZ_AI` stays ON as in every official package (D-039).

## 5. Known gaps and risks

- **Not run on a real desktop.** CI proves build, install and a first window under a virtual display. Wayland, GPU, portals, the keyring and a card reader still need a person.
- **Qt and Frameworks versions.** `CMakeLists.txt` asks for Qt 6.11 and KF 6.29. `kf6-core24` is built by KDE neon and its stable channel may be older (the `kf6-core24-sdk` of June 2026 had Qt 6.11.1 and KF 6.27). If CI fails with "Could not find KF6 6.29", either the stable content snap is waited for or the minimum used for the snap is lowered; that is a decision for the maintainer, not made here.
- **Layout paths.** AqBanking's plug-in folders are bound with `layout`. If a library installs under a multiarch folder, the install check's library listing shows it, and the layout is fixed.
- **Online build.** The snap build downloads crates and rustup; a store-side "remote build" is not used. Reproducibility comes from `Cargo.lock` and the pinned tarball checksums.
- **Snapcraft is not pinned** yet (`THIRD_PARTY.md`).

## 6. Channels

`edge` for main builds, `beta` for pre-releases, `stable` at Gold (issue #892). Only the pre-release part is built: `release.yml` uploads a tag's snap to `beta` when the store login exists. A main build is **not** uploaded, because that would publish without the maintainer's tag (§3.2); proposed in section 8.

## 7. Canonical's terms

The Snap Store terms (canonical.com) could not be read from this build environment (the host is outside its network allow-list), so **the legal check of #892 is still open**: the maintainer reads the Snap Store Terms of Service and the publisher rules and tells us if anything conflicts with GPL-3.0-or-later, AI-made packaging or this project's rules. The packaging itself contains nothing beyond a free-software snap.

## 8. What needs the maintainer

1. A Snapcraft (Ubuntu One) account and the registration of the snap's name. The name in the file, `kassensturz`, is provisional; the store may already hold it.
2. The login as CI secret: run `snapcraft export-login --snaps kassensturz --channels beta --acls package_access,package_push,package_release -` and store the output as the secret `SNAPCRAFT_STORE_CREDENTIALS` of the environment `release`. Until then the upload job only warns.
3. Optionally a request to the store for auto-connection of `password-manager-service` and `pcscd` (section 4), or leave it manual.
4. Read the terms (section 7).

## Folder checks for the VM rounds

The snap plugs `home` and `removable-media` (Q-570) so a book can be opened where it really is, with locks; both can be switched off. To check in a round on a real desktop, with the snap installed from a main build:

1. First start: the book is made in the app's own folder (`~/snap/kassensturz/common/.local/share/...`, the same for every revision, B-266), backups next to it.
2. New book: the page asks for a *folder* (no save dialog); the chosen folder, e.g. in Documents, works for the book, its lock file and SQLite's journal (D-730, D-733).
3. Import: a file chosen with the file chooser (KMyMoney, CAMT, OFX) is read once; nothing else of the folder is reachable.
4. Export and backup copy: the save dialog writes where the user chose; a chosen folder that the portal no longer grants (reset with `flatpak permission-reset` equivalent: `xdg-document-portal` entries removed) shows the plain "choose the folder again" message.
5. Bill folder (D-739/D-763) and the receipt inbox's portal grants: chosen once, still there after a restart.
6. External drive: a book on a USB stick (not mounted in the snap, only through the portal) opens or is refused with a clear message, never a silent empty book.

Known limit (B-265): the snap's AppArmor profile denies file locks (`flock`, `fcntl`) in the portal's folders (`/run/user/UID/doc/...`), the Flatpak does not. A backup copy into such a folder works anyway, since the copy is made in a private folder and written as plain bytes. A *book* needs the locks: the chooser tries a lock in the real folder (home or removable media connected) and in the portal's path, and refuses the folder with `folder.no-locks` if neither works.

Checks for the interfaces (VM rounds, snap): (a) with `home` connected, choose `~/Documents/Books` for a new book, open it, close it, reopen it, and `ls` shows the book there with its `.lock` while open; (b) `sudo snap disconnect kassensturz:home`, restart: the same choice is refused with the plain message, nothing is created; (c) reconnect, restart: it works again; (d) an already open book in home: disconnect, restart, the app says the folder can't be reached and offers choosing it again, the book file is untouched; (e) with a USB stick: refused until `removable-media` is connected, then works; (f) `snap connections kassensturz` shows both switches in the same state as KDE Discover's permissions page.

Data (B-266): `packaging/snap/launch.sh` points `XDG_DATA_HOME`, `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` to `$SNAP_USER_COMMON`, which every revision shares, so an update or rollback never leaves books or settings behind. Data an earlier revision kept in its own folder is copied once, never moved. The launcher also sets `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1` so Qt offers the accessibility tree (to be confirmed by the VM round).

Open point: the portal's host-path lookup (`GetHostPaths`) needs the snap to reach `org.freedesktop.portal.Documents` on D-Bus. If snapd's `desktop` interface does not allow it, the folder is shown by its name instead of its real path (the existing fallback). The round says which.
