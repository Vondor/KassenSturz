# Why Windows and macOS take extra work

KassenSturz is built for Linux first and then for the others, in this order: KDE Plasma, GNOME, Windows, macOS (AGENTS.md section 2, principle 5). On Linux a release is one tag and one click. Windows and macOS each add steps that no workflow can take over today. None of this is a bug in KassenSturz; these are the rules of the two platforms, and a maintainer should know them before promising anything to testers.

The project uses only free signing options (D-048, D-853). Most of what follows comes from that.

## Linux: the easy one

The release run builds the Flatpak and the snap, checks them, and puts the snap in the Snap Store after your approval. Nothing is uploaded by hand. Packages for Debian, Fedora or other distributions are parked for now (#1267); Flathub is not used (D-765).

Linux is also where the team actually tests: the VM rounds run on Kubuntu, and the release run installs the Flatpak on a fresh Ubuntu machine. If you use Arch, btw, you already knew that and have built it from source.

## Windows

**What works.** The app is in the Microsoft Store (ID `9NM5V48PPRLQ`), certified since 18 Nov 2026. The Store signs the package itself, so Store users get a trusted app with no warning. Each release also has a portable zip and an installer.

**What takes extra work.**

- **The Store upload is manual.** The release workflow already has a job that uploads to Partner Center by itself (#927, D-844), but it signs in through a Microsoft Entra application. A personal Microsoft developer account has no free Entra directory; getting one means a paid Azure or billing sign-up. So the job is switched off (the repository variable `STORE_UPLOAD` is not set), and every version is uploaded by hand in Partner Center ([steps](01-releasing.md#the-microsoft-store-by-hand)). If the project ever has an Entra directory, `docs/WINDOWS.md` section 2 says how to switch it on.
- **Certification takes Microsoft's time.** Every submission goes through Microsoft's certification. It can take from hours to a few business days, and it is out of our hands.
- **The Store keeps the old version until certification passes.** On release day the GitHub release and the snap have the new build while the Store still offers the previous one. Testers who install from the Store (most testers are on Windows) get the new build only after that. Plan test sessions with this gap in mind.
- **Only one submission at a time.** A submission still in certification blocks the next one. If two releases come close together, the second waits.
- **The installer and the zip are unsigned.** Signing an installer needs a code signing certificate. The free route, SignPath Foundation, declined the project in Nov 2026 for lack of public visibility; paid certificates are not used (D-048, D-853). So Windows SmartScreen warns on the first start of the installer and the zip. The Store is the signed route; send testers there.
- **The Store app keeps its book elsewhere.** Windows deletes a Store app's private folders when it is uninstalled, so the Store app keeps the book in `Documents\KassenSturz` (D-856), and its settings backup goes there too (D-945). Testers asking "where is my book?" need this answer.
- **No bank connections in the Windows app yet.** AqBanking and Gwenhywfar are not in KDE Craft's cache for the Windows build, so it has no bank connections (D-554); statements are imported from files.

## macOS

**What works.** Every release gets an experimental `.dmg` for Apple Silicon (M1 or newer), built on a GitHub Mac machine that starts the app once as a check (D-875).

**What takes extra work.**

- **No Apple developer account.** Apple signs and notarizes apps only for members of its Developer Program, which costs 99 USD a year. The project has no account (D-048), so the `.dmg` carries only an ad-hoc signature. Apple Silicon needs that to run the app at all, but it doesn't make macOS trust it.
- **Gatekeeper blocks the first start.** macOS says it can't check the app and doesn't open it. The user has to go to *System Settings → Privacy & Security* and press *Open Anyway*, or remove the quarantine flag in the Terminal. The steps are in the `.dmg` and in [`docs/MACOS.md`](../MACOS.md). Many people stop at that first message, and that is a reasonable thing for them to do.
- **No store.** The Mac App Store needs a signed, sandboxed app; Homebrew needs a signed one too (D-875). So the `.dmg` from the Releases page is the only way.
- **Nobody on the team has a Mac.** No person has ever run the Mac build. It is tested only by the automated start in CI. Expect bugs, and say so to anyone who asks.
- **No bank connections.** AqBanking isn't packaged for macOS in KDE Craft, and an untested bank path that handles a real PIN is worse than none (D-876). File import works.
- **The Mac build doesn't block anything.** Its CI job is not a required check, and a failed Mac build only means the `.dmg` is missing from that release. GitHub's Mac machines are sometimes simply not available; the workflow `rerun-lost-runner.yml` retries those runs.

**What would change it.** An Apple Developer ID (and someone with a Mac to test). Then `packaging/macos/make-dmg.sh` gets the real signature plus notarization, which is a human gate (AGENTS.md section 3.2), and the Gatekeeper steps go away.

## In one table

| | Linux (snap, Flatpak) | Windows (Store) | Windows (zip, installer) | macOS (.dmg) |
|---|---|---|---|---|
| Upload | automatic after your approval | by hand in Partner Center | automatic, on the release | automatic, on the release |
| Signed | by the store / not needed | by Microsoft | no, SmartScreen warns | ad hoc only, Gatekeeper blocks |
| Available after the tag | when the release run ends | after certification: hours to days | when the release run ends | a little after the release |
| Tested by people | yes, VM rounds | yes, VM rounds | yes, VM rounds | no |
| Bank connections | yes | no | no | no |
