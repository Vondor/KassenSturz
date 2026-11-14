# Handling a FinTS product registration number

*Deutsch: [FINTS_PRODUCT_ID.de.md](FINTS_PRODUCT_ID.de.md)*

This note is for developers of other open-source programs that talk to German banks over FinTS (HBCI), for example through AqBanking. It explains how KassenSturz handles its FinTS product registration number, and why, so you can do the same in your own project. It contains no number and no part of one.

It is not legal advice. The terms of **your** registration decide what you may do. When in doubt, ask the registration office before you ship.

## What the number is

Since 2019 banks only answer FinTS programs that are registered with Die Deutsche Kreditwirtschaft. Registration is free, is done once per product (not per version), and takes a few weeks. You receive a product registration number (25 letters and digits). Your program sends it to the bank in every dialog, so the bank knows which product is talking to it. Registration: <https://www.fints.org/de/hersteller/produktregistrierung>.

The number is not a password. Nobody can open a bank account or log in with it. But it identifies your product, and if it is misused, the registration office can block it, which cuts off every user of your product from their bank.

## Rule 1: one product, one number

- **Every product needs its own registration.** Never use a number you found in another program, in its source, in a forum or in a library's examples. That includes KassenSturz's.
- **A changed build is a different product.** As KassenSturz understands the registration office: a finished program shipped unchanged as the registered product may carry the number; a fork or a modified build is another product, and its maker registers their own.
- **Numbers registered for libraries are for internal tests only** (see the FAQ on fints.org). They are not meant for a program shipped to users.

## Rule 2: keep the number out of public source

KassenSturz was told the number must not stand in public source. So no file in the repository holds it, and it reaches only the official release builds. Here is how that is built, in case you want to copy the idea (the code is GPL-3.0-or-later; see `crates/kassensturz-bank/build.rs`):

1. **The source has an empty place.** The program reads the number at compile time with Rust's `option_env!`. Without it, the value is `None`.
2. **The build script takes the number only from outside the repository:** an environment variable, or a file in the repository's root that `.gitignore` lists. It trims spaces, checks the shape (exactly 25 ASCII letters and digits) and hands the number to the compiler as an environment variable (`cargo:rustc-env`). It never writes it into a generated source file, so a debug-source package cannot pick it up.
3. **A wrong shape stops the build** with a message that does not show the value.
4. **Only the release job of a release tag gets the number.** It reads it from a secret that is stored in a protected deployment environment on the code host, writes the git-ignored file right before the build and deletes it right after.
5. **The release job checks its own output.** After the build it searches every file of the finished package and fails unless the number is in the program file and in no other file.
6. **Pull requests prove the check works** by running the same steps with a made-up number of the right shape.

### Protecting the release path

A secret in CI is only as safe as the workflow that can read it. KassenSturz:

- lets only the maintainer create release tags (a tag ruleset);
- keeps the secret in a deployment environment that allows only release tags and asks the maintainer to approve each run;
- never uses `pull_request_target`, so code from a pull request never runs with secrets;
- pins every CI action to a commit;
- saves no build cache in the release job, and never runs with shell tracing (`set -x`) or very verbose Cargo output (`cargo -vv`), which would print compiler environment variables;
- writes no AqBanking message logs, since those contain the dialog the number is part of.

Remember that anyone can read the number out of a released program. Keeping it out of the source does not make it secret; it keeps it from being copied into other products by accident and keeps you within your registration's terms. Have a plan for a blocked number: register again, store the new number, and tag a new release.

## How builds without the number behave

Most builds have no number: builds from source, pull request builds and development builds. They must still work.

- **The whole app works**, including importing bank statement files. Only live bank access is off.
- **The bank screens say plainly** that this build is not registered and what to do instead (import a file, or use an official release).
- **For developers testing against their own bank**, KassenSturz has a developer switch (an environment variable, documented in the README only). With it, the app asks for a number in a password field the first time it would contact a bank, before anything is sent. The number is checked, kept in memory only, wiped when dropped, never logged or saved, and forgotten when the app closes. "Cancel" contacts no bank.
- **Whoever builds KassenSturz with their own registration** puts their own number into the git-ignored file and builds. See "Bank access in your own build" in the README.

## Handing the number to AqBanking

AqBanking takes the number as the runtime setting `fintsRegistrationKey` of an open session:

```c
AB_Banking_RuntimeConfig_SetCharValue(ab, "fintsRegistrationKey", key);
```

AqBanking reads that value each time it writes a message and puts it into the product field of the dialog start. Check the shape yourself before you hand it over, so no FinTS separator character can get into a message.

## What not to do

- Don't put the number in source, a generated file, a test, a fixture, a config template or a default setting.
- Don't put it in a commit message, issue, pull request, chat, prompt, log, screenshot or document, and don't let an AI agent search for it.
- Don't reuse another product's number, and don't give yours to a fork.
- Don't let pull request code, forks or every branch run with the secret.
- Don't print it in build output or in error messages, and don't log FinTS messages that contain it.
- Don't make a build without the number fail or crash; let it work without live bank access and say why.

## Where to read more

- In this repository: decisions D-058, D-260 and D-745 in [`DECISIONS.md`](DECISIONS.md), threat T-63 in [`THREAT_MODEL.md`](THREAT_MODEL.md), [`RELEASING.md`](RELEASING.md), and `crates/kassensturz-bank/build.rs`.
- AqBanking's public source and documentation, for the runtime setting.
- The registration office's FAQ on fints.org.
