# GitHub-only test automation

This workflow supplements the existing `.github/workflows/ci.yml`; it does not replace or weaken any checks.

## Baseline

The existing CI already runs formatting, linting, REUSE/SPDX, DCO, license checks, unit/integration/property/golden tests and cross-platform jobs. The additional `cli-smoke.yml` workflow executes the public CLI against a disposable synthetic SQLite book on GitHub-hosted Ubuntu.

Run it on pull requests that change Rust code or the smoke-test files. It can also be started in Actions using `workflow_dispatch` once the workflow exists on the default branch.

## What the smoke test verifies

- A new test book can be created without a real user's data.
- An account, category and sample transaction can be created.
- The account register reports the sample account.
- Transfer suggestions and a rules dry run exit successfully.
- All sample files are created under a temporary directory and removed afterwards.

This is **not automated GUI coverage** and does not validate exact account balances. Existing domain tests cover money invariants, while a separate GUI runner needs technical and license review before adding dependencies.

## Follow-up automation

Plan automated desktop GUI tests on a GitHub-hosted Linux runner with a virtual display, and preserve diagnostic logs/screenshots as CI artifacts on failures. No local KDE machine, manual clicking or real household data is part of this workflow.

## Reporting

For each failed GitHub Actions run, record the commit SHA, failing job and step, minimal reproduction using synthetic data, and a link to sanitized logs or artifacts.
