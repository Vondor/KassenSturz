<!--
SPDX-FileCopyrightText: 2026 KassenSturz contributors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Fuzzing (D-787)

For agents and developers. A fuzzer feeds a program millions of broken inputs and reports the ones that make it crash, hang or run out of memory. KassenSturz reads files it did not write (bank statements, other programs' books, a book that was damaged on disk), so each of these readers has a fuzz target. A target must never crash on any input: a bad file is refused with an error code, nothing more.

## Targets

| Target | What it reads |
|---|---|
| `camt053` | ISO 20022 CAMT.053 / CAMT.052 statements |
| `mt940` | MT940 statements |
| `ofx` | OFX statements |
| `csv` | CSV statements, with every column mapping option |
| `kmy` | KMyMoney `.kmy` files |
| `kmy_sqlite` | KMyMoney SQLite files |
| `book_open` | a KassenSturz book file, through the normal open path (integrity checks included) |

The targets are in `fuzz/fuzz_targets/`. They only call the public functions of the importers and the store and hold no logic of their own.

## Running them

The fuzz crate is a separate workspace (`fuzz/`, excluded from the main one), so the app, its packages and `Cargo.lock` never contain `libfuzzer-sys`. It needs a nightly Rust and `cargo-fuzz`:

```
cargo install cargo-fuzz --locked --version 0.13.2
cd fuzz
cargo +nightly fuzz run camt053 -- -max_total_time=60
```

Use synthetic seed files only (AGENTS.md §3.1 rule 2). CI takes the importers' own test files as seeds.

## In CI

`.github/workflows/fuzz.yml` runs every target:

- on a pull request that touches an importer, the store, the core or `fuzz/`: a short smoke run (one minute per target). It is not a required check.
- every night: a 30-minute run per target. A crash, hang or out-of-memory fails the job, and the input that caused it is kept as a CI artifact.

A finding is fixed like any bug: add a regression test with the input as a synthetic file, fix the code, add a row to `docs/SECURITY_LOG.md` when it can harm stored data.

## Dependencies

`libfuzzer-sys` is test-only and lives only in `fuzz/Cargo.lock`. Its license (`(MIT OR Apache-2.0) AND NCSA`) is outside the allowlist of D-026, so `fuzz/deny.toml` allows NCSA for that one workspace and CI checks it with a second cargo-deny step. The record is in `docs/THIRD_PARTY.md`.
