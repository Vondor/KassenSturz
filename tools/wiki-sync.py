#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 KassenSturz contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Turns the user handbook in docs/user/de and docs/user/en into GitHub Wiki pages.

The handbook lives in the repository so that every change is reviewed in a
pull request and matches the build a person has (#654). The GitHub Wiki is
only a copy of it, made by .github/workflows/wiki.yml after a merge to main
(D-880). This script writes that copy into an empty folder:

- every page gets a short name with its language in front (`de-05-konten`,
  `en-05-accounts`); the two index pages become `Handbuch` and `Handbook`,
- links between handbook pages point to the Wiki pages, other links into
  the repository point to the file on main, and images are loaded from main,
- every page starts with a notice that the page is a copy, that edits in the
  Wiki are overwritten, and where to change it instead,
- `Home`, `_Sidebar` and `_Footer` are generated.

A link to a file that does not exist makes the script fail, so a broken link
is caught in the pull request (the workflow runs this script there too).

Usage: tools/wiki-sync.py OUTPUT_FOLDER [--repo OWNER/NAME] [--branch main]
"""

import argparse
import posixpath
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LANGUAGES = {
    "de": {
        "index": "Handbuch",
        "title": "Handbuch (Deutsch)",
        "notice": (
            "> **Diese Seite ist eine Kopie.** Sie wird automatisch aus "
            "[`{path}`]({source}) im Repository erzeugt. Änderungen hier im "
            "Wiki werden beim nächsten Abgleich überschrieben. Um die Seite zu "
            "verbessern, ändern Sie bitte die Datei im Repository und öffnen "
            "Sie einen Pull Request; wie das geht, steht in "
            "[CONTRIBUTING.md]({contributing})."
        ),
    },
    "en": {
        "index": "Handbook",
        "title": "Handbook (English)",
        "notice": (
            "> **This page is a copy.** It is made automatically from "
            "[`{path}`]({source}) in the repository. Edits made here in the "
            "Wiki are overwritten the next time it is updated. To improve the "
            "page, please change the file in the repository and open a pull "
            "request; [CONTRIBUTING.md]({contributing}) explains how."
        ),
    },
}

# [text](target) and ![alt](target); the target ends at the first space or ")".
LINK = re.compile(r"(!?)\[([^\]]*)\]\(([^)\s]+)\)")


def page_name(lang: str, path: Path) -> str:
    """The Wiki page name of a handbook file."""
    if path.name == "README.md":
        return LANGUAGES[lang]["index"]
    return f"{lang}-{path.stem}"


def collect_pages() -> dict[str, tuple[str, str]]:
    """Maps each handbook file (repository path) to its language and Wiki page name."""
    pages = {}
    for lang in LANGUAGES:
        for path in sorted((ROOT / "docs" / "user" / lang).glob("*.md")):
            pages[path.relative_to(ROOT).as_posix()] = (lang, page_name(lang, path))
    return pages


def convert(
    source: str,
    text: str,
    pages: dict[str, tuple[str, str]],
    blob: str,
    raw: str,
    errors: list[str],
) -> str:
    """Rewrites the links of one handbook page for the Wiki."""
    folder = posixpath.dirname(source)

    def replace(match: re.Match) -> str:
        image, label, target = match.groups()
        if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("#"):
            return match.group(0)
        path, _, anchor = target.partition("#")
        resolved = posixpath.normpath(posixpath.join(folder, path))
        if resolved.startswith("../") or not (ROOT / resolved).exists():
            errors.append(f"{source}: link to a missing file: {target}")
            return match.group(0)
        suffix = f"#{anchor}" if anchor else ""
        if image:
            return f"![{label}]({raw}/{resolved})"
        if resolved in pages:
            return f"[{label}]({pages[resolved][1]}{suffix})"
        return f"[{label}]({blob}/{resolved}{suffix})"

    return LINK.sub(replace, text)


def first_heading(text: str, fallback: str) -> str:
    for line in text.splitlines():
        if line.startswith("# "):
            return line[2:].strip()
    return fallback


def build(out: Path, repo: str, branch: str) -> list[str]:
    blob = f"https://github.com/{repo}/blob/{branch}"
    raw = f"https://github.com/{repo}/raw/{branch}"
    contributing = f"{blob}/CONTRIBUTING.md"
    pages = collect_pages()
    errors: list[str] = []
    sidebar = {lang: [] for lang in LANGUAGES}

    for source, (lang, name) in pages.items():
        text = (ROOT / source).read_text(encoding="utf-8")
        notice = LANGUAGES[lang]["notice"].format(
            path=source, source=f"{blob}/{source}", contributing=contributing
        )
        body = convert(source, text, pages, blob, raw, errors)
        (out / f"{name}.md").write_text(f"{notice}\n\n{body}", encoding="utf-8")
        if name != LANGUAGES[lang]["index"]:
            sidebar[lang].append((first_heading(text, name), name))

    home = [
        "# KassenSturz",
        "",
        "**Deutsch**",
        "",
        "- [Installieren und aktualisieren](de-02-installieren)",
        "- [Handbuch auf Deutsch](Handbuch)",
        "- [Ein Problem melden](de-19-probleme)",
        "- [Eine KI benutzen (freiwillig)](de-ki)",
        "- [Roadmap: Was als Nächstes kommt](de-22-fahrplan)",
        "",
        "**English**",
        "",
        "- [Install and update](en-02-install)",
        "- [Handbook in English](Handbook)",
        "- [Report a problem](en-19-problems)",
        "- [Using an AI (optional)](en-ai)",
        "",
        "**Für alle / for everyone**",
        "",
        f"- [Discussions: Ankündigungen und Ideen / announcements and ideas](https://github.com/{repo}/discussions)",
        f"- [Für Tester / for testers]({blob}/docs/testing/README.md)",
        "",
        (
            "> **Dieses Wiki ist eine Kopie** des Handbuchs in "
            f"[`docs/user`]({blob}/docs/user) im Repository. Änderungen hier "
            "werden überschrieben; bitte ändern Sie die Dateien dort, mit einem "
            f"Pull Request ([CONTRIBUTING.md]({contributing}))."
        ),
        ">",
        (
            "> **This wiki is a copy** of the handbook in "
            f"[`docs/user`]({blob}/docs/user) in the repository. Edits made here "
            "are overwritten; please change the files there with a pull request "
            f"([CONTRIBUTING.md]({contributing}))."
        ),
        "",
    ]
    # The start page names handbook pages directly; a renamed page must not
    # leave a dead link there.
    names = {name for _, name in pages.values()}
    for line in home:
        for _, _, target in LINK.findall(line):
            if "/" not in target and target not in names:
                errors.append(f"Home: link to a missing Wiki page: {target}")
    (out / "Home.md").write_text("\n".join(home), encoding="utf-8")

    lines = ["**[Home](Home)**", ""]
    for lang, entries in sidebar.items():
        lines += [f"**[{LANGUAGES[lang]['title']}]({LANGUAGES[lang]['index']})**", ""]
        lines += [f"- [{title}]({name})" for title, name in entries]
        lines.append("")
    (out / "_Sidebar.md").write_text("\n".join(lines), encoding="utf-8")

    footer = (
        f"Kopie von / copy of [`docs/user`]({blob}/docs/user). Bitte im "
        "Repository ändern, nicht hier. / Please change it in the repository, "
        "not here.\n"
    )
    (out / "_Footer.md").write_text(footer, encoding="utf-8")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("out", type=Path, help="an empty folder for the Wiki pages")
    parser.add_argument("--repo", default="Vondor/KassenSturz")
    parser.add_argument("--branch", default="main")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    if any(p.suffix == ".md" for p in args.out.iterdir()):
        print(f"{args.out} already has pages; give an empty folder", file=sys.stderr)
        return 2
    errors = build(args.out, args.repo, args.branch)
    for error in errors:
        print(error, file=sys.stderr)
    count = sum(1 for p in args.out.glob("*.md"))
    print(f"wrote {count} Wiki pages to {args.out}")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
