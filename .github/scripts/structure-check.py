#!/usr/bin/env python3
"""Structure check (COMMON.md "Code structure"): every `src/` file has at most
300 lines of non-test code, and no inline test module (`#[cfg(test)] mod x {`).

Test files (`tests.rs`, anything under a `tests/` directory) are not counted.
Exceptions are listed in `.github/structure-allowlist.txt`, one path per line
with a reason after `#`. The list may only shrink: an entry whose file now
complies (or no longer exists) fails the check too, so it gets removed.
"""

import pathlib
import re
import sys

MAX_LINES = 300
ROOT = pathlib.Path(__file__).resolve().parents[2]
ALLOWLIST = ROOT / ".github" / "structure-allowlist.txt"
INLINE = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?mod\s+\w+\s*\{")


def is_test_file(rel: pathlib.PurePath) -> bool:
    return rel.name == "tests.rs" or "tests" in rel.parts[1:-1]


def problems(path: pathlib.Path) -> list[str]:
    """The file's violations: inline test modules, and more than MAX_LINES
    lines outside them."""
    lines = path.read_text(encoding="utf-8").splitlines()
    out = []
    code = 0
    depth = 0  # brace depth inside an inline test module (0: not in one)
    pending_cfg_test = False
    for n, line in enumerate(lines, 1):
        s = line.strip()
        if depth:
            depth += line.count("{") - line.count("}")
            continue
        if s == "#[cfg(test)]":
            pending_cfg_test = True
            code += 1
            continue
        if pending_cfg_test and s.startswith("#["):
            code += 1
            continue
        if pending_cfg_test and INLINE.match(line):
            out.append(f"inline test module at line {n}")
            depth = line.count("{") - line.count("}")
            pending_cfg_test = False
            continue
        pending_cfg_test = False
        code += 1
    if code > MAX_LINES:
        out.append(f"{code} non-test lines (max {MAX_LINES})")
    return out


def main() -> int:
    allowed = {}
    for raw in ALLOWLIST.read_text(encoding="utf-8").splitlines():
        entry = raw.split("#", 1)[0].strip()
        if entry:
            if "#" not in raw or not raw.split("#", 1)[1].strip():
                print(f"{ALLOWLIST.name}: {entry}: no reason given")
                return 1
            allowed[entry] = raw
    failed = False
    seen = set()
    for path in sorted((ROOT / "src").rglob("*.rs")):
        rel = path.relative_to(ROOT)
        if is_test_file(rel):
            continue
        found = problems(path)
        key = rel.as_posix()
        if key in allowed:
            seen.add(key)
            if not found:
                print(f"{key}: complies now; remove it from {ALLOWLIST.name}")
                failed = True
            continue
        for p in found:
            print(f"{key}: {p}")
            failed = True
    for key in sorted(set(allowed) - seen):
        print(f"{key}: in {ALLOWLIST.name} but not a checked file; remove it")
        failed = True
    if not failed:
        print(f"structure ok ({len(allowed)} allowlisted)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
