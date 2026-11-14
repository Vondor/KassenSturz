<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Code signing policy

Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).

This page says which KassenSturz files are signed, who may approve it and how a signed file is made. It is written for the people who install KassenSturz and for SignPath Foundation. **Status:** signing is not switched on yet. Until the SignPath Foundation accepts the project and the maintainer has set it up (the steps are in the Windows packaging notes), the Windows installer is unsigned. The Microsoft Store signs its own package itself (D-048, #927).

## What is signed

- Only the **Windows installer** (`KassenSturz-<version>-windows-x64-setup.exe`) and the programs inside it, as built by the project's GitHub Actions workflow from a **release tag** of this repository.
- Nothing built on a developer's computer, nothing from a fork and nothing from a pull request is signed.
- The signature proves where the file came from: it was built from the public source in this repository by this repository's workflow. It does not prove that the program is free of mistakes.

KassenSturz contains no proprietary component, and every dependency is listed in [`docs/THIRD_PARTY.md`](THIRD_PARTY.md) with its license.

## Who decides

- **Approver:** only the human maintainer of the repository approves a signing request (AGENTS.md §3.2: signing and publishing packages are a human gate). The maintainer is the sole committer, reviewer and approver role in the signing setup.
- **Contributors and AI agents** write code and open pull requests. They never approve a signing request, never merge into `main` on their own for a release and never hold signing keys. The certificate and its private key stay with SignPath; nobody on the project can export them.
- Every member with access to the repository or to SignPath uses **multi-factor authentication**.

## How a signed file is made

1. The change is merged into `main` through a pull request that passed CI, including the build and install checks of the Windows package.
2. The maintainer tags a release. The workflow `.github/workflows/release.yml` builds the Windows installer from that tag on a GitHub-hosted runner, from the public source, with the build scripts in this repository (`packaging/windows/`, `tools/`).
3. The workflow submits the unsigned installer to SignPath. SignPath checks that the file came from this repository, this workflow and this tag (origin verification).
4. The maintainer approves the request in SignPath. Only then is the file signed and attached to the release.

Changes to the build scripts and to the workflow are reviewed like any other source change.

## Privacy

KassenSturz sends nothing about you, your computer or your books anywhere: no telemetry, no analytics, no crash reports and no update checks (AGENTS.md §3.1 rule 4). Network access happens only for features you switch on yourself, such as a bank connection or an AI you connect (AGENTS.md §8). Signing adds no program that talks to a server.

## Reporting a problem

If you think a signed file is not from this project, or a signed file does something it should not, open an issue at <https://github.com/Vondor/KassenSturz/issues> or write to the maintainer through the contact on the repository page. The project helps SignPath Foundation with any check or root-cause analysis it asks for, and will ask for a certificate to be revoked if a signed file turns out to be harmful.
