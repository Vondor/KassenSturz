@AGENTS.md

# Gemini / Antigravity notes

- `AGENTS.md` is the single source of truth. Do not duplicate its rules here.
- Before starting work, read `docs/README.md` (the map of the docs folder), then only what the task needs; look up decisions in `docs/DECISIONS.md` by their D number (D-960).
- Prefer small, reviewable changes. Run formatting, lint, and tests through `tools/sdk.sh` before proposing a commit.
- Sign off every commit under the DCO (`git commit -s`, D-040) and include the Gemini co-author trailer (`Co-authored-by: Gemini <200291788+gemini-code-assist@users.noreply.github.com>`).
- Never quote the maintainer in commits, pull requests, issues, or docs, and never read real personal or financial data.
- Respect all human gates in `AGENTS.md` §3.2 (do not merge gated changes, tag releases, or change dependencies without maintainer approval).
