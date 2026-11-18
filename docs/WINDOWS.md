<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# KassenSturz on Windows: packages and signing

**Status:** the MSIX for the Microsoft Store is built by CI, and a release can upload it to Partner Center by itself once the maintainer has set it up (#927, D-844, section 2). The app is certified and published in the Store since 18 Nov 2026: <https://apps.microsoft.com/detail/9NM5V48PPRLQ>. Publishing and any signing set-up stay the maintainer's steps (AGENTS.md §3.2: signing and publishing packages). Only free signing options are used (D-048).

## 1. The packages

The `Windows app` workflow (`.github/workflows/windows-app.yml`) builds three files from one Craft build and puts them in the `out` folder (and into a release):

| File | For | Signed |
|---|---|---|
| `KassenSturz-<version>-windows-x64.zip` | portable use, testers | no |
| `KassenSturz-<version>-windows-x64-setup.exe` | the installer, direct download | no, see section 3 |
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

For example build `0.261114.3` is MSIX version `1.2611.14003.0`. The order of builds is kept, and it fits while there are fewer than 1000 builds a day. The last part stays `0` because the Store reserves it. **Verified:** Partner Center accepted the first package, build `0.261117.8` as `1.2611.17008.0`, on 17 Nov 2026. If it ever refuses the mapping, the rule here and in the script change together.

## 2. Putting the MSIX in the Store

The first submission was made by hand in Partner Center (17 Nov 2026); the Store CLI cannot make a first submission from a loose MSIX. Every later one can come from the release run.

### Automatically, from the release (#927, D-844)

The job *Upload the Windows package to the Microsoft Store* in `release.yml` runs after the GitHub pre-release is published. It downloads the Microsoft Store Developer CLI (pinned version and SHA-256, its usage data to Microsoft switched off before its first command), signs in to Partner Center with the four secrets below and runs `msstore publish` with the release's `...-store.msix` and the app's Store ID `9NM5V48PPRLQ`. That makes a new submission from the last published one with the new package in it. The job runs in the environment `release`, so it waits for your approval in the run (Review deployments) like the other publishing jobs, and only a `v0.*` tag run can read the secrets. A failed upload leaves the GitHub release as it is.

It is **off** until you set the repository variable `STORE_UPLOAD` (Settings → Secrets and variables → Actions → Variables):

| `STORE_UPLOAD` | What happens |
|---|---|
| not set (default) | the job is skipped; upload by hand as below |
| `draft` | the package goes up as a **draft** submission; you open the app in Partner Center, look at it and press *Submit* (the maintainer's choice, D-844, Q-631) |
| `submit` | the submission goes to Microsoft's certification at once; certification still takes Microsoft's usual time |

An optional variable `STORE_FLIGHT_ID` (a package flight's ID from Partner Center) sends the package to that flight only, i.e. to the testers in its group, instead of to everyone who can see the listing.

**Once: set it up (maintainer).**

1. **Microsoft Entra app.** Partner Center → ⚙ Account settings → User management → *Microsoft Entra applications* → *Create Microsoft Entra application* (or add an existing one), give it the role **Manager**. Note its **tenant ID** and **client ID** (application ID), and create a **new key** (client secret). Pick the key's lifetime on purpose and write its expiry date into this file under "Key expiry"; an expired key makes the job fail with a sign-in error.
2. **Seller ID.** Partner Center → ⚙ Account settings → Legal info → Developer tab: **Seller ID**.
3. **Secrets.** Settings → Environments → `release` → Add environment secret, four times: `STORE_TENANT_ID`, `STORE_CLIENT_ID`, `STORE_CLIENT_SECRET`, `STORE_SELLER_ID`. Type or paste them there and nowhere else. They never go into the repository, an issue or a chat.
4. **Switch it on.** Set the variable `STORE_UPLOAD` to `draft` (or `submit`).

Key expiry: not set up yet.

Microsoft's guide to the CLI in GitHub Actions says app updates this way work for free apps only; KassenSturz is free in every market. If a price is ever set, check this again. Only one submission can be open at a time: if an earlier one is still a draft or in certification, the CLI may fail; finish or delete that one in Partner Center and re-run the job.

### By hand

1. Download `...-store.msix` from the release (or the workflow artifact).
2. In Partner Center open the app "KassenSturz - The Household Budget App", start an update submission and upload the package under "Packages".
3. Check the listing (English and German), age rating and the privacy statement. KassenSturz sends nothing anywhere unless you switch it on (AGENTS.md §3.1 rule 4); say so.
4. The Store signs the package. Nothing else is needed from our side.

The package declares the restricted capability `runFullTrust` (a normal desktop program). Partner Center asks for a short reason: the program is a Qt desktop application that opens and saves the user's own book files.

## 3. A signed installer (SignPath Foundation)

**Status (18 Nov 2026):** SignPath Foundation declined the application: the program is for projects with more public visibility (stars, forks, contributors, outside articles or discussions), and the project may apply again once it has that. Its paid subscription is not used (D-048, only free signing). Until then the Store is the signed route for Windows users, and the installer and zip stay unsigned.

SignPath Foundation signs open-source programs for free. It needs a first public release of the installer, a code signing policy page ([`CODE_SIGNING_POLICY.md`](CODE_SIGNING_POLICY.md)), two-factor sign-in for everyone with access, and an automated build from the public repository. Steps for the maintainer, once the project is accepted:

1. Apply again at <https://signpath.org> with the repository and the policy page (the first application was declined).
2. Install the SignPath GitHub app and create the project and the release-signing policy in SignPath, with the maintainer as the only approver.
3. Add the organisation id and API token as repository secrets (never in the repository).
4. In `release.yml` add a job that uploads the unsigned installer artifact to SignPath, waits for the maintainer's approval and attaches the signed file to the release. This is a change to the release path and is made in its own pull request.

Until then the installer is unsigned, and Windows SmartScreen warns on first start.
