#!/usr/bin/env python3
"""Check local links and balanced Markdown/HTML containers in agent-track chapters."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote


LINK_RE = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")


def check_file(path: Path) -> list[str]:
    errors: list[str] = []
    text = path.read_text(encoding="utf-8")

    if text.count("```") % 2:
        errors.append(f"{path}: unbalanced fenced code block")
    if text.count("<details>") != text.count("</details>"):
        errors.append(f"{path}: unbalanced <details> block")

    for line_number, line in enumerate(text.splitlines(), start=1):
        for raw_target in LINK_RE.findall(line):
            target = raw_target.split(maxsplit=1)[0].strip("<>")
            if not target or target.startswith(("#", "http://", "https://", "mailto:")):
                continue
            local_part = unquote(target.split("#", 1)[0])
            if local_part and not (path.parent / local_part).exists():
                errors.append(f"{path}:{line_number}: missing local link target {target}")

    return errors


def main(argv: list[str]) -> int:
    if not argv:
        print("usage: check_track.py MARKDOWN_FILE [MARKDOWN_FILE ...]", file=sys.stderr)
        return 2

    errors: list[str] = []
    for value in argv:
        path = Path(value)
        if not path.is_file():
            errors.append(f"{path}: file does not exist")
            continue
        errors.extend(check_file(path))

    if errors:
        print("agent-track structural checks failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"agent-track structural checks passed for {len(argv)} file(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
