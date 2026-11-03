# Releasing a pre-release build

How a build for testers is made and published (D-551, D-579). Until the maintainer declares 1.0 every build is a **pre-release** numbered `0.YYMMDD.N` (D-152, D-226). Only the maintainer tags and publishes (AGENTS.md section 3.2); the workflow never creates a tag, and what it makes stays a draft until he publishes it.

## The tag

A release tag is `v` plus the build number of the commit it points to, for example `v0.261103.0`. `tools/build-version.sh` prints the number: the commit's date in UTC and the count of that day's merges on `main` (D-226). The release workflow checks both rules and stops with a message that says what to do:

- the tag equals `v<number of its commit>`, so the name of the release tells a tester which build it is;
- the commit is on `main`, so only merged code that passed CI is released.

No file holds the version: do not edit `Cargo.toml`, `CMakeLists.txt` or the AppStream `<release>` entry for a pre-release (D-226).

## Steps for the maintainer

1. **Choose the commit.** Normally the newest commit on `main` once all its CI checks are green: `git switch main && git pull --ff-only`.
2. **Tag it.** The tag is made on your machine and pushed:

   ```sh
   tag="v$(tools/build-version.sh)"
   git tag -a "$tag" -m "Pre-release ${tag#v}"
   git push origin "$tag"
   ```

3. **Wait for the run.** The **Release** workflow (Actions tab, about 15 minutes with a warm cache) has three jobs:
   - *Build the Flatpak bundle* builds `KassenSturz-<number>-x86_64.flatpak` from the commit, with the number and commit shown in Settings → About.
   - *Install the bundle as the Ubuntu sheet says* takes that file on a fresh Ubuntu machine and runs what `docs/testing/INSTALL_UBUNTU.de.md` tells a tester: install Flatpak, add the catalog and the KDE runtime for the user, install the file, start it with the developer mode and check the debug log was written, uninstall it. (The update over an older build, card 6, can't be checked here: it needs two builds.) Nothing is released if this fails.
   - *Draft pre-release on GitHub* creates the release `KassenSturz <number> (pre-release)` as a **draft** with the files attached and a text that lists the merged changes since the previous tag.
4. **Look at the draft.** On the repository's Releases page open the draft: the file name, the text, and that *Set as a pre-release* is ticked.
5. **Publish it.** Press *Publish release*. From then on the file can be downloaded without a GitHub account, and the install sheets' step 1 ("the newest entry marked Pre-release") is right.
6. **Tell the facilitator** the exact file name (the sheets say the facilitator names it before the session).

## If something is wrong

- The run is red: read the failing job. If the fault is in the code or the workflow, fix it on `main` first. A tag that did not produce a release is deleted and the number of the fixed commit is tagged instead: `git push --delete origin "$tag" && git tag -d "$tag"`.
- A published release has a wrong file: delete the release on GitHub (not only the draft), delete the tag as above, and tag again. Testers who already downloaded the file are told through the facilitator.
- The same tag may be re-run (re-run the workflow in the Actions tab): files of an existing release are replaced and its text is kept.

## What else goes in a release (slots in `release.yml`)

- **Windows installer (M7a.7).** A job of this workflow builds it (for example by calling `windows-app.yml` as a reusable workflow) and uploads the installer as an artifact whose name starts with `release-asset-`; adding that job to the `needs` list of the release job attaches the file next to the Flatpak bundle.
- **Example books for the build month (M7a.8).** A step before the Flatpak build runs the example book generator with the build month as its reference month, so "this month" and "last month" are never empty in a tester's copy.

Both are marked `SLOT` in the workflow file.

## Trying the workflow without a tag

A pull request that changes `.github/workflows/release.yml` or `tools/build-version.sh` runs the **Release** workflow's build and install check (not the draft), so a change to the release path is proven before a tag depends on it. The caches are read from `main`: the builder's folder has the same key as the Flatpak job of `ci.yml` (`flatpak-builder-state-x86_64-<hash of packaging/flatpak/**>`), which only `main` saves.

## What a tester's file is not

It is not signed, and it is not on Flathub (M7b). The Windows installer is unsigned too and shows SmartScreen's warning (D-542).
