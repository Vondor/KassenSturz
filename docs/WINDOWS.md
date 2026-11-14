<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# KassenSturz on Windows: packages and signing

**Status:** the MSIX for the Microsoft Store is built by CI (#927). Uploading it and the SignPath wiring are the maintainer's steps (AGENTS.md §3.2: signing and publishing packages). Only free signing options are used (D-048).

## 1. The packages

The `Windows app` workflow (`.github/workflows/windows-app.yml`) builds three files from one Craft build and puts them in the `out` folder (and into a release):

| File | For | Signed |
|---|---|---|
| `KassenSturz-<version>-windows-x64.zip` | portable use, testers | no |
| `KassenSturz-<version>-windows-x64-setup.exe` | the installer, direct download | not yet, see section 3 |
| `KassenSturz-<version>-windows-x64-store.msix` | the Microsoft Store | no: the Store signs it itself |

The MSIX holds the same programs as the zip (only `kassensturz.exe` and `kassensturz-mcp.exe`, B-292) plus `AppxManifest.xml` (`packaging/windows/msix/`) and three logos made from the app icon. CI packs it with `makeappx`, unpacks it again and checks the package name, the publisher, the version and that `bin\kassensturz.exe` is inside.

### The Store identity

The values in `packaging/windows/msix/AppxManifest.xml` come from the maintainer's Partner Center reservation. They are public (anyone can read them from a downloaded package) and are not secrets:

- Name `47425SerpentSoftware.KassenSturz-TheHouseholdBudge`
- Publisher `CN=734D2F8D-959F-4381-AD8C-FF9F9E9C279C`
- Display name "KassenSturz - The Household Budget App", publisher display name "SerpentSoftware"

If the reservation changes, change the manifest and the two checks at the end of `packaging/windows/msix/build-msix.ps1` together.

### The version

A build is numbered `MAJOR.YYMMDD.N` (D-152). An MSIX version has four parts of at most 65535, and a first part of `0` may not be accepted (unverified), so `build-msix.ps1` maps it:

`MAJOR.YYMMDD.N` becomes `(MAJOR+1).YYMM.(DD*1000+N).0`

For example build `0.261114.3` is MSIX version `1.2611.14003.0`. The order of builds is kept, and it fits while there are fewer than 1000 builds a day. The last part stays `0` because the Store reserves it. **Not yet verified against Partner Center:** if it refuses the mapping, the rule here and in the script change together.

## 2. Putting the MSIX in the Store (maintainer)

1. Download `...-store.msix` from the release (or the workflow artifact).
2. In Partner Center open the app "KassenSturz - The Household Budget App", start a submission and upload the package under "Packages".
3. Fill in the listing (English and German), age rating and the privacy statement. KassenSturz sends nothing anywhere unless you switch it on (AGENTS.md §3.1 rule 4); say so.
4. The Store signs the package. Nothing else is needed from our side.

The package declares the restricted capability `runFullTrust` (a normal desktop program). Partner Center asks for a short reason: the program is a Qt desktop application that opens and saves the user's own book files.

## 3. A signed installer (SignPath Foundation)

SignPath Foundation signs open-source programs for free. It needs a first public release of the installer, a code signing policy page ([`CODE_SIGNING_POLICY.md`](CODE_SIGNING_POLICY.md)), two-factor sign-in for everyone with access, and an automated build from the public repository. Steps for the maintainer, once the project is accepted:

1. Apply at <https://signpath.org> with the repository and the policy page.
2. Install the SignPath GitHub app and create the project and the release-signing policy in SignPath, with the maintainer as the only approver.
3. Add the organisation id and API token as repository secrets (never in the repository).
4. In `release.yml` add a job that uploads the unsigned installer artifact to SignPath, waits for the maintainer's approval and attaches the signed file to the release. This is a change to the release path and is made in its own pull request.

Until then the installer is unsigned, and Windows SmartScreen warns on first start.
