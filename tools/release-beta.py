#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Helper for beta GitHub Releases (D-987, extending D-948).

Provides three CLI modes using only the Python 3 standard library:

  1. ``notes``: generates English-only beta release notes (D-981) with a
     cumulative ``--first-parent`` changelog since the latest stable GitHub
     Release that is an ancestor of ``<tag>^`` (falling back to
     ``git describe --tags --abbrev=0 --match 'v0.*' <tag>^``).
  2. ``superseded-betas``: prints older ``(beta)`` pre-release tags that are
     ancestors of ``<tag>``, oldest-first, so the release workflow can remove
     superseded beta GitHub Releases while keeping git tags intact.
  3. ``--self-test``: verifies baseline selection, cumulative notes, beta
     cleanup filtering, JSON normalization, and edge cases in synthetic
     temporary git repositories.
"""

from __future__ import annotations

import argparse
import dataclasses
import datetime
import json
import pathlib
import subprocess
import sys
import tempfile
from typing import Any


BETA_NAME_SUFFIX = " (beta)"


@dataclasses.dataclass(frozen=True)
class Release:
    tag_name: str
    name: str
    is_prerelease: bool
    published_at: str


def _parse_timestamp(value: str) -> float:
    """Return a POSIX timestamp for ISO 8601 strings, or -inf if empty/invalid."""
    if not value:
        return float("-inf")
    text = value.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        dt = datetime.datetime.fromisoformat(text)
    except ValueError:
        return float("-inf")
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=datetime.timezone.utc)
    return dt.timestamp()


def _published_sort_key(release: Release) -> tuple[float, str, str]:
    return (_parse_timestamp(release.published_at), release.published_at, release.tag_name)


def normalize_releases(raw: Any) -> list[Release]:
    """Normalize GitHub REST API (paginated or flat) and ``gh release list`` JSON.

    Draft releases are always skipped.
    """
    if not isinstance(raw, list):
        raise ValueError("releases JSON must be a list of release objects or pages")

    flat_entries: list[Any] = []
    for item in raw:
        if isinstance(item, list):
            flat_entries.extend(item)
        else:
            flat_entries.append(item)

    releases: list[Release] = []
    for entry in flat_entries:
        if not isinstance(entry, dict):
            raise ValueError("each release entry must be a JSON object")

        is_draft = bool(
            entry["draft"] if "draft" in entry else entry.get("isDraft", False)
        )
        if is_draft:
            continue

        tag_name = entry["tag_name"] if "tag_name" in entry else entry.get("tagName")
        if not isinstance(tag_name, str) or not tag_name:
            continue

        raw_name = entry.get("name")
        name = raw_name if isinstance(raw_name, str) else ""

        is_prerelease = bool(
            entry["prerelease"]
            if "prerelease" in entry
            else entry.get("isPrerelease", False)
        )

        raw_published = (
            entry["published_at"]
            if "published_at" in entry
            else entry.get("publishedAt")
        )
        published_at = raw_published if isinstance(raw_published, str) else ""

        releases.append(
            Release(
                tag_name=tag_name,
                name=name,
                is_prerelease=is_prerelease,
                published_at=published_at,
            )
        )

    return releases


def load_releases(path: pathlib.Path) -> list[Release]:
    data = json.loads(path.read_text(encoding="utf-8"))
    return normalize_releases(data)


def _run_git(args: list[str], repo_dir: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=repo_dir,
        capture_output=True,
        text=True,
        check=False,
    )


def resolve_tag_commit(tag: str, repo_dir: pathlib.Path) -> str | None:
    """Resolve ``refs/tags/<tag>^{commit}`` to a commit SHA, or ``None``."""
    proc = _run_git(["rev-parse", "--verify", "--quiet", f"refs/tags/{tag}^{{commit}}"], repo_dir)
    if proc.returncode != 0:
        return None
    commit = proc.stdout.strip()
    return commit or None


def resolve_parent_commit(tag: str, repo_dir: pathlib.Path) -> str | None:
    """Resolve the first parent commit of ``<tag>``, or ``None`` if root commit."""
    commit = resolve_tag_commit(tag, repo_dir)
    if commit is None:
        raise RuntimeError(f"git tag does not resolve to a commit: {tag}")
    proc = _run_git(["rev-parse", "--verify", "--quiet", f"{commit}^"], repo_dir)
    if proc.returncode != 0:
        return None
    parent = proc.stdout.strip()
    return parent or None


def is_ancestor(ancestor_commit: str, descendant_commit: str, repo_dir: pathlib.Path) -> bool:
    proc = _run_git(["merge-base", "--is-ancestor", ancestor_commit, descendant_commit], repo_dir)
    return proc.returncode == 0


def select_baseline_tag(
    tag: str,
    releases: list[Release],
    repo_dir: pathlib.Path,
) -> str | None:
    """Pick the newest stable ancestor of ``<tag>^``, or fall back to ``git describe``."""
    parent_commit = resolve_parent_commit(tag, repo_dir)
    if parent_commit is None:
        return None

    stable_candidates = sorted(
        (r for r in releases if not r.is_prerelease and r.tag_name != tag),
        key=_published_sort_key,
        reverse=True,
    )
    for release in stable_candidates:
        cand_commit = resolve_tag_commit(release.tag_name, repo_dir)
        if cand_commit is not None and is_ancestor(cand_commit, parent_commit, repo_dir):
            return release.tag_name

    describe = _run_git(
        ["describe", "--tags", "--abbrev=0", "--match", "v0.*", parent_commit],
        repo_dir,
    )
    if describe.returncode == 0:
        fallback = describe.stdout.strip()
        if fallback:
            return fallback
    return None


def find_superseded_betas(
    tag: str,
    releases: list[Release],
    repo_dir: pathlib.Path,
) -> list[str]:
    """Return older ``(beta)`` pre-release tags that are ancestors of ``<tag>``.

    Results are sorted oldest-first by ``published_at`` for deterministic cleanup.
    """
    tag_commit = resolve_tag_commit(tag, repo_dir)
    if tag_commit is None:
        raise RuntimeError(f"git tag does not resolve to a commit: {tag}")

    beta_candidates = sorted(
        (
            r
            for r in releases
            if r.is_prerelease
            and r.name.endswith(BETA_NAME_SUFFIX)
            and r.tag_name != tag
        ),
        key=_published_sort_key,
    )

    superseded: list[str] = []
    seen: set[str] = set()
    for release in beta_candidates:
        if release.tag_name in seen:
            continue
        cand_commit = resolve_tag_commit(release.tag_name, repo_dir)
        if cand_commit is not None and is_ancestor(cand_commit, tag_commit, repo_dir):
            superseded.append(release.tag_name)
            seen.add(release.tag_name)
    return superseded


def render_notes(
    repo: str,
    tag: str,
    version: str,
    sha: str,
    baseline: str | None,
    repo_dir: pathlib.Path,
) -> str:
    """Render the English-only beta release notes (D-981)."""
    lines = [
        f"Build {version} of KassenSturz. It may still have bugs: use made-up numbers or an example book only, never your own data.",
        "",
        f"How to install it: follow the install sheet for your system in [docs/testing](https://github.com/{repo}/tree/{tag}/docs/testing).",
        "",
        f"Windows: the signed version is in the [Microsoft Store](https://apps.microsoft.com/detail/9NM5V48PPRLQ). The installer and the zip here are not signed, so Windows shows a SmartScreen warning first. See [the code signing policy](https://github.com/{repo}/blob/{tag}/docs/CODE_SIGNING_POLICY.md).",
        "",
        f"macOS (experimental, untested): a `.dmg` for Macs with an Apple chip is added to this release a little later, if its build worked. It is not signed or notarized by Apple, so macOS blocks it first. Nobody on the team has a Mac to test it on. Read [docs/MACOS.md](https://github.com/{repo}/blob/{tag}/docs/MACOS.md) before you open it.",
        "",
        f"Commit: `{sha}`",
    ]

    if baseline:
        baseline_commit = resolve_tag_commit(baseline, repo_dir)
        tag_commit = resolve_tag_commit(tag, repo_dir)
        if baseline_commit is None:
            raise RuntimeError(f"baseline git tag does not resolve to a commit: {baseline}")
        if tag_commit is None:
            raise RuntimeError(f"git tag does not resolve to a commit: {tag}")
        log_proc = _run_git(
            ["log", "--first-parent", "--format=- %s", f"{baseline_commit}..{tag_commit}"],
            repo_dir,
        )
        if log_proc.returncode != 0:
            raise RuntimeError(f"git log failed: {log_proc.stderr.strip()}")
        changelog = log_proc.stdout.rstrip("\n")
        if changelog:
            lines.extend(["", f"Changes since {baseline}:", "", changelog])

    return "\n".join(lines) + "\n"


def _git_checked(args: list[str], repo_dir: pathlib.Path) -> str:
    proc = _run_git(args, repo_dir)
    if proc.returncode != 0:
        raise RuntimeError(
            f"git {' '.join(args)} failed ({proc.returncode}): {proc.stderr.strip()}"
        )
    return proc.stdout.strip()


def _init_synthetic_repo(repo_dir: pathlib.Path) -> None:
    _git_checked(["init", "-b", "main"], repo_dir)
    _git_checked(["config", "user.name", "Example Contributor"], repo_dir)
    _git_checked(["config", "user.email", "contributor@example.invalid"], repo_dir)


def _commit_and_tag(
    repo_dir: pathlib.Path,
    filename: str,
    content: str,
    subject: str,
    tag: str | None = None,
    annotated: bool = False,
) -> str:
    (repo_dir / filename).write_text(content, encoding="utf-8")
    _git_checked(["add", filename], repo_dir)
    _git_checked(["commit", "-m", subject], repo_dir)
    sha = _git_checked(["rev-parse", "HEAD"], repo_dir)
    if tag is not None:
        if annotated:
            _git_checked(["tag", "-a", tag, "-m", f"Release {tag}"], repo_dir)
        else:
            _git_checked(["tag", tag], repo_dir)
    return sha


def run_self_test() -> int:
    """Run deterministic self-tests against synthetic temporary git repositories."""
    with tempfile.TemporaryDirectory(prefix="release-beta-test-") as tmp:
        root = pathlib.Path(tmp)

        # Scenario 1 & 2 & 5: Multiple betas above stable, re-run on older beta,
        # and REST API paginated vs gh release list JSON normalization.
        repo1 = root / "repo1"
        repo1.mkdir()
        _init_synthetic_repo(repo1)

        _commit_and_tag(repo1, "notes.txt", "1\n", "feat: initial ledger skeleton", "v0.260101.0")
        _commit_and_tag(
            repo1,
            "notes.txt",
            "2\n",
            "feat: add overview summary",
            "v0.260102.0",
            annotated=True,
        )
        _commit_and_tag(repo1, "notes.txt", "3\n", "fix: round split display", "v0.260103.0")
        _commit_and_tag(
            repo1,
            "notes.txt",
            "4\n",
            "feat: add category search",
            "v0.260104.0",
            annotated=True,
        )
        sha5 = _commit_and_tag(
            repo1,
            "notes.txt",
            "5\n",
            "fix: preserve filter on refresh",
            "v0.260105.0",
            annotated=True,
        )

        rest_pages = [
            [
                {
                    "tag_name": "v0.260105.0",
                    "name": "KassenSturz 0.260105.0 (beta)",
                    "prerelease": True,
                    "draft": False,
                    "published_at": "2026-01-05T12:00:00Z",
                },
                {
                    "tag_name": "v0.260104.0",
                    "name": "KassenSturz 0.260104.0 (beta)",
                    "prerelease": True,
                    "draft": False,
                    "published_at": "2026-01-04T12:00:00Z",
                },
                {
                    "tag_name": "v0.260104.0",
                    "name": "KassenSturz 0.260104.0 (stable)",
                    "prerelease": False,
                    "draft": True,
                    "published_at": "2026-01-04T15:00:00Z",
                },
            ],
            [
                {
                    "tag_name": "v0.260103.0",
                    "name": "KassenSturz 0.260103.0 (beta)",
                    "prerelease": True,
                    "draft": False,
                    "published_at": "2026-01-03T12:00:00Z",
                },
                {
                    "tag_name": "v0.260102.0",
                    "name": "KassenSturz 0.260102.0 (stable)",
                    "prerelease": False,
                    "draft": False,
                    "published_at": "2026-01-02T12:00:00Z",
                },
                {
                    "tag_name": "v0.260101.0",
                    "name": "KassenSturz 0.260101.0 (pre-release)",
                    "prerelease": True,
                    "draft": False,
                    "published_at": "2026-01-01T12:00:00Z",
                },
                {
                    "tag_name": "v0.260101.0",
                    "name": "KassenSturz 0.260101.0 (beta)",
                    "prerelease": True,
                    "draft": True,
                    "published_at": "2026-01-01T13:00:00Z",
                },
            ],
        ]
        gh_list = [
            {
                "tagName": "v0.260105.0",
                "name": "KassenSturz 0.260105.0 (beta)",
                "isPrerelease": True,
                "isDraft": False,
                "publishedAt": "2026-01-05T12:00:00Z",
            },
            {
                "tagName": "v0.260104.0",
                "name": "KassenSturz 0.260104.0 (beta)",
                "isPrerelease": True,
                "isDraft": False,
                "publishedAt": "2026-01-04T12:00:00Z",
            },
            {
                "tagName": "v0.260104.0",
                "name": "KassenSturz 0.260104.0 (stable)",
                "isPrerelease": False,
                "isDraft": True,
                "publishedAt": "2026-01-04T15:00:00Z",
            },
            {
                "tagName": "v0.260103.0",
                "name": "KassenSturz 0.260103.0 (beta)",
                "isPrerelease": True,
                "isDraft": False,
                "publishedAt": "2026-01-03T12:00:00Z",
            },
            {
                "tagName": "v0.260102.0",
                "name": "KassenSturz 0.260102.0 (stable)",
                "isPrerelease": False,
                "isDraft": False,
                "publishedAt": "2026-01-02T12:00:00Z",
            },
            {
                "tagName": "v0.260101.0",
                "name": "KassenSturz 0.260101.0 (pre-release)",
                "isPrerelease": True,
                "isDraft": False,
                "publishedAt": "2026-01-01T12:00:00Z",
            },
            {
                "tagName": "v0.260101.0",
                "name": "KassenSturz 0.260101.0 (beta)",
                "isPrerelease": True,
                "isDraft": True,
                "publishedAt": "2026-01-01T13:00:00Z",
            },
        ]

        rest_path = root / "rest.json"
        gh_path = root / "gh.json"
        rest_path.write_text(json.dumps(rest_pages), encoding="utf-8")
        gh_path.write_text(json.dumps(gh_list), encoding="utf-8")

        rels_rest = load_releases(rest_path)
        rels_gh = load_releases(gh_path)
        assert rels_rest == rels_gh, f"JSON normalization mismatch: {rels_rest} != {rels_gh}"
        assert len(rels_rest) == 5, f"expected 5 non-draft releases, got {len(rels_rest)}"

        baseline = select_baseline_tag("v0.260105.0", rels_rest, repo1)
        assert baseline == "v0.260102.0", f"expected baseline v0.260102.0, got {baseline}"

        notes = render_notes(
            repo="example-org/kassensturz",
            tag="v0.260105.0",
            version="0.260105.0",
            sha=sha5,
            baseline=baseline,
            repo_dir=repo1,
        )
        assert "Build 0.260105.0 of KassenSturz." in notes
        assert f"Commit: `{sha5}`" in notes
        assert "Changes since v0.260102.0:" in notes
        assert "- fix: preserve filter on refresh" in notes
        assert "- feat: add category search" in notes
        assert "- fix: round split display" in notes
        assert "- feat: add overview summary" not in notes
        assert "- feat: initial ledger skeleton" not in notes

        superseded = find_superseded_betas("v0.260105.0", rels_rest, repo1)
        assert superseded == ["v0.260103.0", "v0.260104.0"], (
            f"expected ['v0.260103.0', 'v0.260104.0'], got {superseded}"
        )

        # Scenario 2: Re-running on an older beta tag never deletes a newer beta.
        superseded_oldest = find_superseded_betas("v0.260103.0", rels_rest, repo1)
        assert superseded_oldest == [], (
            f"expected [] when re-running oldest beta, got {superseded_oldest}"
        )
        superseded_middle = find_superseded_betas("v0.260104.0", rels_rest, repo1)
        assert superseded_middle == ["v0.260103.0"], (
            f"expected ['v0.260103.0'] when re-running middle beta, got {superseded_middle}"
        )

        # Scenario 3: No stable release yet -> falls back to git describe; also root commit.
        repo2 = root / "repo2"
        repo2.mkdir()
        _init_synthetic_repo(repo2)
        sha_root = _commit_and_tag(repo2, "a.txt", "1\n", "feat: first commit", "v0.260201.0")
        _commit_and_tag(repo2, "a.txt", "2\n", "feat: second commit", "v0.260202.0")

        no_stable_releases = normalize_releases(
            [
                {
                    "tag_name": "v0.260201.0",
                    "name": "KassenSturz 0.260201.0 (pre-release)",
                    "prerelease": True,
                    "draft": False,
                    "published_at": "2026-02-01T10:00:00Z",
                }
            ]
        )
        assert select_baseline_tag("v0.260202.0", no_stable_releases, repo2) == "v0.260201.0"
        assert select_baseline_tag("v0.260201.0", [], repo2) is None
        root_notes = render_notes(
            repo="example-org/kassensturz",
            tag="v0.260201.0",
            version="0.260201.0",
            sha=sha_root,
            baseline=None,
            repo_dir=repo2,
        )
        assert "Changes since" not in root_notes

        # Scenario 4: Missing tag and non-ancestor stable tag are safely skipped.
        repo3 = root / "repo3"
        repo3.mkdir()
        _init_synthetic_repo(repo3)
        _commit_and_tag(repo3, "b.txt", "1\n", "feat: base stable", "v0.260301.0")
        _git_checked(["checkout", "-b", "side"], repo3)
        _commit_and_tag(repo3, "side.txt", "side\n", "feat: side branch stable", "v0.260302.0")
        _commit_and_tag(repo3, "side.txt", "side2\n", "fix: side branch beta", "v0.260302.1")
        _git_checked(["checkout", "main"], repo3)
        _commit_and_tag(repo3, "b.txt", "2\n", "feat: main beta", "v0.260303.0")

        tricky_releases = normalize_releases(
            [
                {
                    "tag_name": "v0.260399.0",
                    "name": "KassenSturz 0.260399.0 (stable)",
                    "prerelease": False,
                    "draft": False,
                    "published_at": "2026-03-09T10:00:00Z",
                },
                {
                    "tag_name": "v0.260302.0",
                    "name": "KassenSturz 0.260302.0 (stable)",
                    "prerelease": False,
                    "draft": False,
                    "published_at": "2026-03-02T10:00:00Z",
                },
                {
                    "tag_name": "v0.260301.0",
                    "name": "KassenSturz 0.260301.0 (stable)",
                    "prerelease": False,
                    "draft": False,
                    "published_at": "2026-03-01T10:00:00Z",
                },
                {
                    "tag_name": "v0.260398.0",
                    "name": "KassenSturz 0.260398.0 (beta)",
                    "prerelease": True,
                    "draft": False,
                    "published_at": "2026-03-01T11:00:00Z",
                },
                {
                    "tag_name": "v0.260302.1",
                    "name": "KassenSturz 0.260302.1 (beta)",
                    "prerelease": True,
                    "draft": False,
                    "published_at": "2026-03-02T11:00:00Z",
                },
            ]
        )
        assert select_baseline_tag("v0.260303.0", tricky_releases, repo3) == "v0.260301.0"
        assert find_superseded_betas("v0.260303.0", tricky_releases, repo3) == []

        # Scenario 6: Idempotency when no superseded betas exist.
        single_beta_releases = normalize_releases(
            [
                {
                    "tag_name": "v0.260105.0",
                    "name": "KassenSturz 0.260105.0 (beta)",
                    "prerelease": True,
                    "draft": False,
                    "published_at": "2026-01-05T12:00:00Z",
                },
                {
                    "tag_name": "v0.260102.0",
                    "name": "KassenSturz 0.260102.0 (stable)",
                    "prerelease": False,
                    "draft": False,
                    "published_at": "2026-01-02T12:00:00Z",
                },
            ]
        )
        assert find_superseded_betas("v0.260105.0", single_beta_releases, repo1) == []

    print("release-beta.py self-test: 6/6 scenarios passed")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Generate cumulative beta release notes and find superseded beta releases."
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run built-in self-tests in temporary synthetic git repositories.",
    )
    subparsers = parser.add_subparsers(dest="command")

    notes_parser = subparsers.add_parser(
        "notes",
        help="Generate release notes since the latest stable ancestor release.",
    )
    notes_parser.add_argument("--repo", required=True, help="GitHub repository (owner/repo).")
    notes_parser.add_argument("--tag", required=True, help="Git tag being released.")
    notes_parser.add_argument("--version", required=True, help="Build version string.")
    notes_parser.add_argument("--sha", required=True, help="Full commit SHA for the tag.")
    notes_parser.add_argument(
        "--releases-json",
        required=True,
        type=pathlib.Path,
        help="Path to JSON from GitHub REST API or gh release list.",
    )
    notes_parser.add_argument(
        "--repo-dir",
        default=pathlib.Path("."),
        type=pathlib.Path,
        help="Path to the git repository (default: current directory).",
    )
    notes_parser.add_argument(
        "--output",
        type=pathlib.Path,
        default=None,
        help="Write rendered release notes to this path (default: stdout).",
    )

    superseded_parser = subparsers.add_parser(
        "superseded-betas",
        help="List older (beta) pre-release tags that are ancestors of --tag.",
    )
    superseded_parser.add_argument("--tag", required=True, help="Git tag being released.")
    superseded_parser.add_argument(
        "--releases-json",
        required=True,
        type=pathlib.Path,
        help="Path to JSON from GitHub REST API or gh release list.",
    )
    superseded_parser.add_argument(
        "--repo-dir",
        default=pathlib.Path("."),
        type=pathlib.Path,
        help="Path to the git repository (default: current directory).",
    )

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.self_test:
        return run_self_test()

    if args.command == "notes":
        releases = load_releases(args.releases_json)
        baseline = select_baseline_tag(args.tag, releases, args.repo_dir)
        text = render_notes(
            repo=args.repo,
            tag=args.tag,
            version=args.version,
            sha=args.sha,
            baseline=baseline,
            repo_dir=args.repo_dir,
        )
        if args.output is not None:
            args.output.write_text(text, encoding="utf-8")
        else:
            sys.stdout.write(text)
        return 0

    if args.command == "superseded-betas":
        releases = load_releases(args.releases_json)
        for old_tag in find_superseded_betas(args.tag, releases, args.repo_dir):
            print(old_tag)
        return 0

    parser.print_usage(sys.stderr)
    print(
        "error: specify a subcommand ('notes', 'superseded-betas') or '--self-test'",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
