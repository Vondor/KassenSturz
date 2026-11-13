# Releasing a pre-release build

How a build for testers is made and published (D-551, D-579). Until the maintainer declares 1.0 every build is a **pre-release** numbered `0.YYMMDD.N` (D-152, D-226). Only the maintainer tags and publishes (AGENTS.md section 3.2); the workflow never creates a tag, and the tag he pushes is his decision to publish: the release appears on the public Releases page when the run ends (D-585, no draft step).

## The tag

A release tag is `v` plus the build number of the commit it points to, for example `v0.261103.0`. `tools/build-version.sh` prints the number: the commit's date in UTC and the count of that day's merges on `main` (D-226). The release workflow checks both rules and stops with a message that says what to do:

- the tag equals `v<number of its commit>`, so the name of the release tells a tester which build it is;
- the commit is on `main`, so only merged code that passed CI is released.

No file holds the version: do not edit `Cargo.toml`, `CMakeLists.txt` or the AppStream `<release>` entry for a pre-release (D-226). The build stamps its own number and date into the newest `<release>` of the AppStream file it installs (`cmake/StampMetainfo.cmake`, M6-B156), so `flatpak info` and software centres show the real number; the file in the repository keeps its last hand-set entry.

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
   - *Pre-release on GitHub* creates the release `KassenSturz <number> (pre-release)`, marked as a pre-release, with the files attached and a text that lists the merged changes since the previous tag.
4. **Look at the release.** On the repository's Releases page the new entry is marked *Pre-release* and can be downloaded without a GitHub account, so the install sheets' step 1 ("the newest entry marked Pre-release") is right. Check the file name and the text.
5. **Tell the facilitator** the exact file name (the sheets say the facilitator names it before the session).

## Once: the registration number

An official release carries KassenSturz's FinTS product registration number, so its users need only their bank login and PIN (D-745). The number is in no file of the repository. It is a secret in the repository's settings on GitHub, and only the maintainer sets it up, once:

1. **Tag ruleset.** Settings → Rules → Rulesets → New tag ruleset: enforcement "Active", target tags `v*` (so later `v1.*` tags are covered too), tick "Restrict creations", "Restrict updates", "Restrict deletions" and "Block force pushes", and put only "Repository admin" on the bypass list.
2. **Environment.** Settings → Environments → New environment, named `release`. Tick "Required reviewers" and add yourself; leave "Prevent self-review" off (otherwise you could not approve your own tag); untick "Allow administrators to bypass configured protection rules"; then "Save protection rules". Under "Deployment branches and tags" choose "Selected branches and tags" and add the tag rule `v*`.
3. **Secret.** In that environment: Add environment secret, name `FINTS_REGISTRATION`, value the 25 characters of the number. Type or paste it there and nowhere else.

From then on the *Build the Flatpak bundle* job of a tag's run waits for your approval (Actions tab → the run → Review deployments), builds the number in, and fails if the number is in any file of the built app other than the program itself. Without the secret a release is still built, without bank access, and the run shows a warning.

These settings are the protection, not the workflow file: a tag runs the `release.yml` of the commit it points to (threat T-63). They hold fully only when nobody else, and no agent, acts with the maintainer's own GitHub account.

**If the number ever has to be replaced** (for example because the registration office blocked it): store the new number as the secret and tag a new release. Nothing in the repository changes.

## If something is wrong

- The run is red: read the failing job. If the fault is in the code or the workflow, fix it on `main` first. A tag that did not produce a release is deleted and the number of the fixed commit is tagged instead: `git push --delete origin "$tag" && git tag -d "$tag"`.
- A published release has a wrong file: delete the release on GitHub, delete the tag as above, and tag again. Testers who already downloaded the file are told through the facilitator.
- The same tag may be re-run (re-run the workflow in the Actions tab): files of an existing release are replaced and its text is kept.

## What goes in a release

- **The Flatpak bundle**, built and installed by the Ubuntu sheet in the same run.
- **The Windows app (M7a.7, D-554):** `KassenSturz-<number>-windows-x64.zip` (portable, no installation and no administrator rights) and `KassenSturz-<number>-windows-x64-setup.exe` (installer). The `windows` job calls `windows-app.yml` with `release: true`, which uploads both as `release-asset-windows`; the release waits for it. With Craft's cache warm it takes about 20 to 35 minutes, cold about 45. Both files are unsigned (D-048), so SmartScreen warns.

What is still to come, marked `SLOT` in the workflow file:

- **Example books for the build month (M7a.8).** A step before the Flatpak build runs the example book generator with the build month as its reference month, so "this month" and "last month" are never empty in a tester's copy.

## Trying the workflow without a tag

A pull request that changes `.github/workflows/release.yml` or `tools/build-version.sh` runs the **Release** workflow's build and install check (not the release), so a change to the release path is proven before a tag depends on it. The caches are read from `main`: the builder's folder has the same key as the Flatpak job of `ci.yml` (`flatpak-builder-state-x86_64-<hash of packaging/flatpak/**>`), which only `main` saves.

## What a tester's file is not

It is not signed, and it is not on Flathub (M7b). The Windows installer is unsigned too and shows SmartScreen's warning (D-542).
