# Tagging and publishing a release

This is the short path from a green commit to a published build. Every step, and what to do when one fails, is in [`docs/RELEASING.md`](../RELEASING.md); the Windows and Snap details are in [`docs/WINDOWS.md`](../WINDOWS.md) and [`docs/SNAP.md`](../SNAP.md).

## The three kinds of build (D-948)

| Kind | What it is | GitHub release | Snap Store |
|---|---|---|---|
| **stable** | the build everyone should use | `KassenSturz <number> (stable)`, marked **Latest**, not a pre-release, label *None* | `stable` and `beta` |
| **beta** | a test build for people who want the newest | `KassenSturz <number> (beta)`, marked **Pre-release** | `beta` only |
| **devel** | every build CI makes from `main` | none | none |

**Gold** is not a kind of build yet: it is the name kept for 1.0, which only the maintainer declares (AGENTS.md section 1). Until then every number starts with `0.`.

You don't pick the kind with the tag. The tag is the same for both; the kind follows from the snap channel you approve in step 4.

Releases made before 21 Nov 2026 are titled `(pre-release)`; they came before these kinds and are left as they are. Every tag's release starts as `(beta)`, marked Pre-release. Approving `snap-stable` turns it into `(stable)`, marked Latest; if the edit fails, the run says so and [`docs/RELEASING.md`](../RELEASING.md) has the steps by hand.

## Step by step

1. **Pick the commit.** Normally the newest commit on `main` whose checks are all green (the green tick next to it on the repository's start page). On your own computer:

   ```sh
   git switch main && git pull --ff-only
   ```

2. **Tag it and push the tag.**

   ```sh
   tag="v$(tools/build-version.sh)"
   git tag -a "$tag" -m "Release ${tag#v}"
   git push origin "$tag"
   ```

   The number is the commit's date and that day's count of merges on `main` (D-152, D-226), for example `v0.261120.39`. No file is edited for it. The workflow stops if the tag doesn't match its commit's number or if the commit isn't on `main`.

3. **Approve the build.** Actions tab → the **Release** run → **Review deployments** → tick **release** → **Approve and deploy**. This lets the build read the FinTS registration number, which lives only in that environment. The run then builds the Flatpak, the Windows files and the snap, installs the Flatpak on a fresh Ubuntu machine, and publishes the GitHub release with all files and a list of the merged changes. The experimental macOS `.dmg` is added a little later if the Mac build worked; if it didn't, the release is still fine.

4. **Choose the snap channel (D-940).** The run now waits on two jobs. **Review deployments** again:
   - **snap-stable** for a stable release: the snap goes to `stable` and `beta`, the store page is updated from `packaging/snap/snapcraft.yaml` (D-947), and the GitHub release becomes `(stable)`, marked Latest.
   - **snap-beta** for a beta: the snap goes to `beta` only, and the release stays `(beta)`.

   Approve one, then open **Review deployments** once more and **Reject** the other. The rejected job shows red; that is expected and changes nothing. Never approve both: the store refuses the same file twice.

5. **Look at the release** on the Releases page: the title says `(stable)` and *Latest* or `(beta)` and *Pre-release*, the files are there, the text reads right.

6. **Microsoft Store, by hand.** See the next section. This is the one step no workflow does for you today.

7. **Tell people.** A stable release gets an announcement in Discussions.

## Later: moving a beta to stable (D-941)

A beta that has proven itself can become the stable release without a new tag or a new build, so stable users get the very file beta testers had.

1. Actions tab → **Promote snap to stable** → **Run workflow** (from `main`). Type the version, for example `0.261120.31`, or leave it empty to take whatever is on `beta` now.
2. **Review deployments** → **snap-stable** → **Approve and deploy**.

The workflow moves the revision that is on `beta` to `stable`, updates the store page, and says in its summary what was on stable before. It stops and changes nothing if the version you typed isn't the one on `beta`. Only the build on `beta` right now can be promoted. It also turns that build's GitHub release into the stable one, the same way as step 4.

## The Snap Store page (D-947)

The summary, title, description, license and icon come from `packaging/snap/snapcraft.yaml` and are sent with every stable upload and every promotion. To send them without a release: Actions tab → **Update Snap Store page** → **Run workflow** (empty version means the newest release) → approve **snap-stable**. Nothing is built and no channel changes.

The category, the links and the screenshots are not sent by snapcraft at all. They are edited on snapcraft.io, in the browser.

**Once:** the store login saved in the environment `snap-stable` needs the `package_metadata` permission. A login made before 21 Nov 2026 doesn't have it; make a new one with the command in [`docs/RELEASING.md`](../RELEASING.md#once-the-snap-environments-d-940) and replace the secret. Without it the page update only warns.

## The Microsoft Store, by hand

The release carries `KassenSturz-<number>-windows-x64-store.msix`. Each new version is uploaded in Partner Center:

1. Download the `...-store.msix` from the GitHub release.
2. Partner Center → the app *KassenSturz - The Household Budget App* → **Start update** (a new submission based on the last one).
3. **Packages**: upload the `.msix`. Leave the other sections as they are unless something changed.
4. If a text changed, update the store listing (English and German). Red banners in Partner Center while you edit have turned out to be harmless before; the submit page says what is really missing.
5. **Submit for certification.**

The MSIX version isn't the build number: `0.YYMMDD.N` becomes `1.YYMM.(DD×1000+N).0`, so `0.261120.39` is `1.2611.20039.0` (`docs/WINDOWS.md`, section 1). Microsoft then certifies the package, which can take from a few hours to a few business days. **Until it passes, the Store keeps offering the previous version.** Nothing about the GitHub release or the snap waits for this.

Why this step is still manual is on the next page: [Why Windows and macOS take extra work](02-platforms.md).

## If something goes wrong

- **The run is red:** read the failing job. Fix the cause on `main`, delete the tag (`git push --delete origin "$tag" && git tag -d "$tag"`) and tag the fixed commit.
- **A published release has a wrong file:** delete the release on GitHub, delete the tag as above, tag again.
- **Only the Mac job failed:** nothing to do for the release; the `.dmg` is just missing.
- **A job waits for approval you didn't give:** GitHub cancels it after 30 days. Nothing reaches a store without your click.
