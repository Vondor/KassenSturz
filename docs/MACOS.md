<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# KassenSturz on macOS: the experimental download

**Status: experimental and untested (D-875).** Nobody on the team owns a Mac or has a Mac virtual machine, so no person has ever run this build. The only check is an automated start of the app on a macOS machine that GitHub provides (`.github/workflows/macos-app.yml`): the app opens its welcome screen without a window and keeps running for 30 seconds. Expect bugs. Please use an example book only, never your own data, and report what you find as an issue.

## What it is

Every tagged pre-release (`v0.*`, see `docs/RELEASING.md`) gets a file `KassenSturz-<version>-macos-arm64-experimental.dmg` on its Releases page, a little after the release itself. If the Mac build failed, the file is missing and the release is unaffected.

- **Apple Silicon only** (M1 or newer). There is no Intel build.
- **Not signed by Apple and not notarized.** That needs an Apple Developer account (99 USD a year), which the project does not have (D-048). The app carries only an ad-hoc signature, which Apple Silicon needs to run it at all; it does not make macOS trust the app.
- **No bank connections.** AqBanking is not part of this build, as in the Windows app (D-054).
- Saved banking PINs would go to the macOS Keychain, but this has never been tried.
- The macOS Mac App Store and Homebrew both need a signed app, so there is no store for this build.

## How to open it

1. Open the `.dmg` and drag **KassenSturz** onto **Applications**.
2. Start KassenSturz. macOS says it cannot check the app and does not open it. Click **Done**.
3. Open **System Settings > Privacy & Security**.
4. Scroll down to the message that KassenSturz was blocked and click **Open Anyway**.
5. Enter your Mac password and confirm with **Open**. This is needed only the first time.

If you prefer the Terminal: `xattr -dr com.apple.quarantine /Applications/KassenSturz.app`.

The same text, in German and English, is in the `.dmg` as `SO OEFFNEN - HOW TO OPEN.txt`.

## For the team

- `packaging/macos/craft/kassensturz/kassensturz.py` is the Craft recipe: Qt 6 and KDE Frameworks come from Craft's binary cache, as for Windows. The Craft and blueprint commits are the same as in `windows-app.yml` and are moved together.
- `packaging/macos/make-dmg.sh` builds `KassenSturz.app` from Craft's folder (`macdeployqt` copies the frameworks, plugins and QML modules), checks that no path of the build machine is linked, signs ad hoc and makes the `.dmg`.
- The app links Breeze's icons and KDE's colour schemes on macOS, as on Windows (D-555, D-581): `CMakeLists.txt`, `build.rs` and `platform.cpp`.
- The workflow runs for a change to those files, on a manual start and on a tag. It is not a required check. A failed Mac build never stops a release.
- **Moving to a signed build:** with an Apple Developer ID, replace the ad-hoc `codesign` in `make-dmg.sh` by the Developer ID signature and add `notarytool` and `stapler`; the signing is then a human gate (AGENTS.md §3.2).
