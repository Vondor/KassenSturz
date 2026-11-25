# Releasing a build

How a build for testers is made and published (D-551, D-579). Until the maintainer declares 1.0 every build is numbered `0.YYMMDD.N` (D-152, D-226), and each release is either a **beta** or a **stable** release (D-948, section "Release kinds" below). Only the maintainer tags and publishes (AGENTS.md section 3.2); the workflow never creates a tag, and the tag he pushes is his decision to publish: the release appears on the public Releases page when the run ends (D-585, no draft step).

## The tag

A release tag is `v` plus the build number of the commit it points to, for example `v0.261103.0`. `tools/build-version.sh` prints the number: the commit's date in UTC and the count of that day's merges on `main` (D-226). The release workflow checks both rules and stops with a message that says what to do:

- the tag equals `v<number of its commit>`, so the name of the release tells a tester which build it is;
- the commit is on `main`, so only merged code that passed CI is released.

**The snap's channel is chosen in the run, not by the tag (D-940, replaces D-939).** Every tag's run has two snap jobs, *Snap Store, beta channel* and *Snap Store, stable and beta channels*, each waiting for your approval in its own environment (`snap-beta`, `snap-stable`). You approve the one you want and reject the other (step 4 below); the tag itself is the same for both.

No file holds the version: do not edit `Cargo.toml`, `CMakeLists.txt` or the AppStream `<release>` entry for a pre-release (D-226). The build stamps its own number and date into the newest `<release>` of the AppStream file it installs (`cmake/StampMetainfo.cmake`, M6-B156), so `flatpak info` and software centres show the real number; the file in the repository keeps its last hand-set entry.

## Steps for the maintainer

1. **Choose the commit.** Normally the newest commit on `main` once all its CI checks are green: `git switch main && git pull --ff-only`.
2. **Tag it.** The tag is made on your machine and pushed:

   ```sh
   tag="v$(tools/build-version.sh)"
   git tag -a "$tag" -m "Release ${tag#v}"
   git push origin "$tag"
   ```

3. **Wait for the run.** The **Release** workflow (Actions tab, about 15 minutes with a warm cache) has three jobs:
   - *Build the Flatpak bundle* builds `KassenSturz-<number>-x86_64.flatpak` from the commit, with the number and commit shown in Settings → About.
   - *Install the bundle as the Ubuntu sheet says* takes that file on a fresh Ubuntu machine and runs what way B of `docs/testing/INSTALL_UBUNTU.de.md` (the Flatpak file) tells a tester: install Flatpak, add the catalog and the KDE runtime for the user, install the file, start it with the developer mode and check the debug log was written, uninstall it. (The update over an older build can't be checked here, since it needs two builds; the automatic update tests cover it, D-835.) Nothing is released if this fails.
   - *Beta release on GitHub* creates the release `KassenSturz <number> (beta)`, marked as a pre-release, with the files attached and a cumulative changelog (`Changes since <tag>:`) built by `tools/release-beta.py` from the latest `(stable)` release whose tag is an ancestor of the new tag (falling back to the previous `v0.*` tag only if no stable release exists yet), then removes superseded older `(beta)` GitHub Releases while keeping git tags in place (D-987, see "Release kinds" below). It becomes the stable release in step 4 if you choose stable.
   A fourth job, *Attach the experimental macOS download*, adds `KassenSturz-<number>-macos-arm64-experimental.dmg` to the release after it exists, only if the Mac build worked (D-875, `docs/MACOS.md`). A red *macOS app* job in the run does not affect the release; the file is just missing.
4. **Choose the snap's channel.** Once the GitHub release is out, the run shows "Waiting for review" for both snap jobs. Actions tab → the run → **Review deployments**: tick **snap-beta** for a test build (the snap goes to `beta`) or **snap-stable** for the stable 0.x release (it goes to `stable` and `beta`, D-879), add a comment if you like, and press **Approve and deploy**. Your choice is also the kind of release (D-948): with **snap-stable**, once the snap is in the store, the job renames the GitHub release to `KassenSturz <number> (stable)`, takes off the *Pre-release* mark and marks it *Latest*; with **snap-beta** it stays `(beta)` and *Pre-release*. Then open **Review deployments** again, tick the other one and press **Reject**. The rejected job shows as failed; that is expected and changes nothing, since the GitHub release is already published. Approve only one: the store refuses the same file twice. A job you leave waiting is cancelled by GitHub after 30 days.
5. **Look at the release.** On the repository's Releases page the new entry is marked *Pre-release* (beta) or *Latest* (stable) and can be downloaded without a GitHub account. Check the title, the file name and the text.
6. **Tell the facilitator** the exact file name (the sheets say the facilitator names it before the session).

## Once: the registration number

An official release carries KassenSturz's FinTS product registration number, so its users need only their bank login and PIN (D-745). The number is in no file of the repository. It is a secret in the repository's settings on GitHub, and only the maintainer sets it up, once:

1. **Tag ruleset.** Settings → Rules → Rulesets → New tag ruleset: enforcement "Active", target tags `v*` (so later `v1.*` tags are covered too), tick "Restrict creations", "Restrict updates", "Restrict deletions" and "Block force pushes", and put only "Repository admin" on the bypass list.
2. **Environment.** Settings → Environments → New environment, named `release`. Tick "Required reviewers" and add yourself; leave "Prevent self-review" off (otherwise you could not approve your own tag); untick "Allow administrators to bypass configured protection rules"; then "Save protection rules". Under "Deployment branches and tags" choose "Selected branches and tags" and add the tag rule `v*`.
3. **Secret.** In that environment: Add environment secret, name `FINTS_REGISTRATION`, value the 25 characters of the number. Type or paste it there and nowhere else.

From then on the *Build the Flatpak bundle* job of a tag's run waits for your approval (Actions tab → the run → Review deployments), builds the number in, and fails if the number is in any file of the built app other than the program itself. Without the secret a release is still built, without bank access, and the run shows a warning.

## Once: the snap environments (D-940)

The two snap jobs need two environments, which only the maintainer sets up, once. For each of `snap-beta` and `snap-stable`:

1. Settings → Environments → **New environment**, name `snap-beta` (then, the second time, `snap-stable`), **Configure environment**.
2. Under *Deployment protection rules*: tick **Required reviewers** and add yourself (`Vondor`); leave **Prevent self-review** unticked (otherwise you could not approve your own tag); untick **Allow administrators to bypass configured protection rules**; press **Save protection rules**.
3. Under *Deployment branches and tags*: choose **Selected branches and tags** → **Add deployment branch or tag rule** → Ref type **Tag**, name pattern `v*` → **Add rule**.
4. Under *Environment secrets*: **Add environment secret**, name `SNAPCRAFT_STORE_CREDENTIALS`, value the output of the login command for that environment, made on your own computer and pasted there and nowhere else:
   - `snap-beta`: `snapcraft export-login --snaps kassensturz --channels beta --acls package_access,package_push,package_release -`
   - `snap-stable`: `snapcraft export-login --snaps kassensturz --channels stable,beta --acls package_access,package_push,package_release,package_metadata -` (`package_metadata` lets it update the store page, D-947) (the Snap Store terms are accepted, `docs/SNAP.md` section 7). An environment without its secret makes the job upload nothing and only warn.

Both environments were set up on 20 Nov 2026.

The old snap login in the environment `release` has been deleted (20 Nov 2026); if the repository variable `SNAP_CHANNELS` (Settings → Secrets and variables → Actions → Variables) still exists, it is no longer read and can go too.

A tag's build also switches on the setup wizard's *Connect to my bank* choice (Q-562, D-546): the workflow writes the file `wizard-bank` before the build, and `CMakeLists.txt` turns it into the Cargo feature `wizard-bank`. A pull request's run of the workflow and CI's own builds don't write it, so they don't offer it. The file holds nothing secret and is not the registration number.

These settings are the protection, not the workflow file: a tag runs the `release.yml` of the commit it points to (threat T-63). They hold fully only when nobody else, and no agent, acts with the maintainer's own GitHub account.

**If the number ever has to be replaced** (for example because the registration office blocked it): store the new number as the secret and tag a new release. Nothing in the repository changes.

## Release kinds (D-948)

There are two kinds of release, and the snap channel you approve picks the kind, so nothing is edited by hand:

| Kind | GitHub release | Snap Store |
|---|---|---|
| **stable** | `KassenSturz <number> (stable)`, marked *Latest*, not a pre-release | `stable` and `beta` |
| **beta** | `KassenSturz <number> (beta)`, marked *Pre-release* | `beta` |

What is merged on `main` without a tag is the **devel** state: CI builds it, and nothing is published (no GitHub release, no snap channel; D-845). **Gold** is the name kept for 1.0, which only you declare.

A beta that is promoted later with *Promote snap to stable* (next section) becomes the stable release on GitHub too. If that relabelling fails, the run warns and you can do it by hand: on the release's page → **Edit**, change the title's `(beta)` to `(stable)`, untick *Set as a pre-release*, tick *Set as the latest release*, **Update release**.

**Only one active beta at a time (D-987).** When `.github/workflows/release.yml` publishes a new `(beta)` release, `tools/release-beta.py` builds its changelog (`Changes since <tag>:`) from the latest `(stable)` release whose tag is an ancestor of the new tag (falling back to the previous `v0.*` tag only if no stable release exists yet), so each new beta inherits every change from any superseded betas and already carries the full changelog if promoted to `(stable)`. After publishing, the workflow removes superseded older `(beta)` GitHub Releases (`gh release delete --yes`, matching pre-releases titled `(beta)` whose tag is an ancestor of the new tag). Git tags stay in place (the `FinTs_Registration` tag ruleset, threat T-63, restricts `refs/tags/v*` deletions to repository admins), and historical pre-D-948 `(pre-release)` releases are left untouched. If deleting a superseded beta release fails, the workflow emits a warning and keeps the newly published beta release; the old entry can be deleted by hand on the Releases page.

## Promoting a beta to stable (D-941)

A build that has been on `beta` for a while can go to `stable` without a new tag or a new build: the workflow **Promote snap to stable** (`.github/workflows/snap-promote.yml`) moves the very snap that beta testers have.

1. Actions tab → **Promote snap to stable** → **Run workflow**. Keep *Use workflow from* on `main`. In *Version* type the version you want on stable, for example `0.261120.31` or `v0.261120.31`, or leave it empty to take whatever is on `beta` now. Press **Run workflow**.
2. The run waits for you (environment `snap-stable`): open it → **Review deployments** → tick **snap-stable** → **Approve and deploy**.
3. It reads what the Snap Store has on `beta` and `stable` and then:
   - promotes that revision to `stable` with `snapcraft release` and says in the run summary which version is on stable now and what was there before;
   - makes that version's GitHub release the stable one: `(stable)` in the title, not a pre-release, *Latest* (D-948);
   - stops without changing anything if the version you typed is not the one on `beta` (the summary names the version that is), if `beta` is empty, or if the store login is missing;
   - does nothing if that version is already on `stable`.

Only the revision on `beta` right now can be promoted. To bring an older one to stable, it has to be on `beta` again first.

**Once, for this workflow:** the environment `snap-stable` only lets `v*` tags in (step 3 of the section above), and this workflow runs from `main`. Settings → Environments → `snap-stable` → *Deployment branches and tags* → **Add deployment branch or tag rule** → Ref type **Branch**, name pattern `main` → **Add rule**. The required review stays, so nothing reaches the store without your approval.

## Updating the Snap Store page (D-947)

The store page follows `packaging/snap/snapcraft.yaml`: its summary, title, description, license and icon replace whatever was edited on snapcraft.io. The category, the links and the screenshots are not sent by snapcraft at all, so they stay as they are on the web and are still edited there.

- **Automatically:** the job *Snap Store, stable and beta channels* of a release run and the workflow *Promote snap to stable* update the page from that version's snap after it reaches `stable`. If that fails, the run only warns, since the snap is already released.
- **By hand:** Actions tab → **Update Snap Store page** → **Run workflow**, with the version whose texts you want (empty: the newest release), then approve **snap-stable** under *Review deployments*. Nothing is built and no channel changes. The texts are those of that release's commit, so a text changed on `main` reaches the page with the next release.

**Once:** the login in `snap-stable` needs the `package_metadata` permission. A login made before 21 Nov 2026 lacks it: make a new one with the `snap-stable` command in "Once: the snap environments" above, and replace the secret (Settings → Environments → `snap-stable` → `SNAPCRAFT_STORE_CREDENTIALS` → edit, paste the new value there and nowhere else). Without it the store refuses the update and the run says so.

## If something is wrong

- The run is red: read the failing job. If the fault is in the code or the workflow, fix it on `main` first. A tag that did not produce a release is deleted and the number of the fixed commit is tagged instead: `git push --delete origin "$tag" && git tag -d "$tag"`.
- A published release has a wrong file: delete the release on GitHub, delete the tag as above, and tag again. Testers who already downloaded the file are told through the facilitator.
- The same tag may be re-run (re-run the workflow in the Actions tab): files of an existing release are replaced and its text is kept.

## What goes in a release

- **The Flatpak bundle**, built and installed by the Ubuntu sheet in the same run.
- **The Windows app (M7a.7, D-554):** `KassenSturz-<number>-windows-x64.zip` (portable, no installation and no administrator rights) and `KassenSturz-<number>-windows-x64-setup.exe` (installer). The job *Windows packages from main's build* looks for `main`'s own *Windows app* run of the tagged commit and hands its packages to the release as `release-asset-windows`: they are the same files with the same number, already started and installed by that run, so they are not built twice (D-957). Only when there is no such run (the tagged merge changed documentation only, `main`'s build failed or is still running, or its files are gone) does the `windows` job call `windows-app.yml` with `release: true` and build them; the release waits for it, about 20 to 35 minutes with Craft's cache warm, cold about 45. The run's summary says which of the two happened. Both files are unsigned (D-048), so SmartScreen warns. The release also carries `KassenSturz-<number>-windows-x64-store.msix` for the Microsoft Store; once the Store upload is set up, the job *Upload the Windows package to the Microsoft Store* puts it into Partner Center after the release is published (it waits for your approval like the build; `docs/WINDOWS.md`, section 2, #927).

What is still to come, marked `SLOT` in the workflow file:

- **Example books for the build month (M7a.8).** A step before the Flatpak build runs the example book generator with the build month as its reference month, so "this month" and "last month" are never empty in a tester's copy.

## Trying the workflow without a tag

A pull request that changes `.github/workflows/release.yml`, `tools/build-version.sh` or `tools/release-beta.py` runs the **Release** workflow's build and install check (not the release), so a change to the release path is proven before a tag depends on it. The caches are read from `main`: the builder's folder has the same key as the Flatpak job of `ci.yml` (`flatpak-builder-state-x86_64-<hash of packaging/flatpak/**>`), which only `main` saves.

## What a tester's file is not

It is not signed. There is no Flathub submission (D-765); Linux users get the snap (`docs/SNAP.md`) or this Flatpak bundle as a direct download. The Windows installer is unsigned too and shows SmartScreen's warning (D-542); the signed way on Windows is the Microsoft Store (`docs/WINDOWS.md`).
