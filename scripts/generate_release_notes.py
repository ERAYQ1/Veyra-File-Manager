#!/usr/bin/env python3
"""Generate categorized GitHub Release notes from Conventional Commits.

Reads commit subjects either from `git log <from>..<to>` or from a plain
text file (one subject per line, via --commits-file — used by tests and
for offline previews), buckets them by Conventional Commits type, and
prints ready-to-paste Markdown for a GitHub Release body.

Usage:
    generate_release_notes.py --version 0.2.0 --from v0.1.0 --to v0.2.0
    generate_release_notes.py --version 0.2.0 --commits-file commits.txt
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass, field

CATEGORY_ORDER = [
    "feat",
    "perf",
    "sec",
    "pkg",
    "test",
    "fix",
    "other",
]

CATEGORY_TITLES = {
    "feat": "🚀 New Features",
    "perf": "⚡ Performance & Scale",
    "sec": "🛡️ Security & Privacy",
    "pkg": "📦 Packaging & Distros",
    "test": "🧪 Testing & Quality",
    "fix": "🐛 Bug Fixes",
    "other": "🔧 Other Changes",
}

TYPE_TO_CATEGORY = {
    "feat": "feat",
    "perf": "perf",
    "sec": "sec",
    "security": "sec",
    "privacy": "sec",
    "pkg": "pkg",
    "packaging": "pkg",
    "test": "test",
    "fix": "fix",
}

# type(scope)!: subject   OR   type: subject
CONVENTIONAL_COMMIT_RE = re.compile(r"^([a-zA-Z]+)(\([^)]*\))?!?:\s*(.+)$")


@dataclass
class CategorizedNotes:
    buckets: dict[str, list[str]] = field(default_factory=dict)

    def add(self, category: str, subject: str) -> None:
        self.buckets.setdefault(category, []).append(subject)


def categorize_commit(subject: str) -> tuple[str, str]:
    """Return (category, cleaned subject) for one commit subject line."""
    match = CONVENTIONAL_COMMIT_RE.match(subject.strip())
    if not match:
        return "other", subject.strip()
    commit_type, scope, rest = match.groups()
    category = TYPE_TO_CATEGORY.get(commit_type.lower(), "other")
    scope_label = scope.strip("()") if scope else None
    cleaned = f"**{scope_label}:** {rest}" if scope_label else rest
    return category, cleaned


def categorize_all(subjects: list[str]) -> CategorizedNotes:
    notes = CategorizedNotes()
    for subject in subjects:
        subject = subject.strip()
        if not subject:
            continue
        category, cleaned = categorize_commit(subject)
        notes.add(category, cleaned)
    return notes


def commits_from_git(rev_range: str) -> list[str]:
    result = subprocess.run(
        ["git", "log", rev_range, "--pretty=format:%s", "--no-merges"],
        check=True,
        capture_output=True,
        text=True,
    )
    return [line for line in result.stdout.splitlines() if line.strip()]


def commits_from_file(path: str) -> list[str]:
    with open(path, encoding="utf-8") as handle:
        return [line for line in handle.read().splitlines() if line.strip()]


def render_markdown(
    version: str,
    notes: CategorizedNotes,
    checksum_file: str | None = None,
    checksum_sha256: str | None = None,
) -> str:
    lines = [f"# Veyra v{version}", ""]

    any_section = False
    for category in CATEGORY_ORDER:
        items = notes.buckets.get(category)
        if not items:
            continue
        any_section = True
        lines.append(f"## {CATEGORY_TITLES[category]}")
        lines.append("")
        for item in items:
            lines.append(f"- {item}")
        lines.append("")

    if not any_section:
        lines.append("_No categorized changes in this release._")
        lines.append("")

    if checksum_file and checksum_sha256:
        lines.append("## 🔒 Checksums")
        lines.append("")
        lines.append("| File | SHA-256 |")
        lines.append("| --- | --- |")
        lines.append(f"| `{checksum_file}` | `{checksum_sha256}` |")
        lines.append("")

    lines.append("## 📥 Quick Install")
    lines.append("")
    lines.append("```bash")
    lines.append(f"curl -LO https://github.com/ERAYQ1/Veyra-File-Manager/releases/download/v{version}/veyra-v{version}-x86_64-linux.tar.gz")
    lines.append(f"tar -xzf veyra-v{version}-x86_64-linux.tar.gz")
    lines.append(f"cd veyra-v{version}-x86_64-linux && ./veyra")
    lines.append("```")
    lines.append("")

    return "\n".join(lines)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, help="Release version, e.g. 0.2.0")
    parser.add_argument("--from", dest="from_ref", help="Starting git ref/tag (exclusive)")
    parser.add_argument("--to", dest="to_ref", default="HEAD", help="Ending git ref/tag (default: HEAD)")
    parser.add_argument("--commits-file", help="Read commit subjects from a plain text file instead of git log")
    parser.add_argument("--checksum-file", help="Name of the release tarball for the checksum table")
    parser.add_argument("--checksum-sha256", help="SHA-256 checksum of --checksum-file")
    parser.add_argument("--output", help="Write to this file instead of stdout")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)

    if args.commits_file:
        subjects = commits_from_file(args.commits_file)
    elif args.from_ref:
        subjects = commits_from_git(f"{args.from_ref}..{args.to_ref}")
    else:
        print("error: either --commits-file or --from must be provided", file=sys.stderr)
        return 2

    notes = categorize_all(subjects)
    markdown = render_markdown(
        args.version,
        notes,
        checksum_file=args.checksum_file,
        checksum_sha256=args.checksum_sha256,
    )

    if args.output:
        with open(args.output, "w", encoding="utf-8") as handle:
            handle.write(markdown)
    else:
        print(markdown)

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
