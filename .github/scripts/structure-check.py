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
INLINE = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?mod\s+(\w+)\s*\{")
# Any cfg whose predicate names `test`: cfg(test), cfg(all(test, unix)), cfg(any(test, ...)).
TEST_CFG = re.compile(r"^#\[cfg\(.*\btest\b")


def is_test_file(rel: pathlib.PurePath) -> bool:
    return rel.name == "tests.rs" or "tests" in rel.parts[1:-1]


def problems_in(lines: list[str]) -> list[str]:
    """The violations in a file's lines: inline test modules (any inline
    `mod tests {`, and any inline module under a cfg naming `test`), and more
    than MAX_LINES lines outside them."""
    out = []
    code = 0
    depth = 0  # brace depth inside an inline test module (0: not in one)
    pending_test_cfg = False
    for n, line in enumerate(lines, 1):
        s = line.strip()
        if depth:
            depth += line.count("{") - line.count("}")
            continue
        code += 1
        if TEST_CFG.match(s):
            pending_test_cfg = True
            continue
        if pending_test_cfg and (not s or s.startswith(("//", "#["))):
            continue  # blank, comment or another attribute: the cfg still applies
        m = INLINE.match(line)
        if m and (pending_test_cfg or m.group(3) == "tests"):
            out.append(f"inline test module at line {n}")
            code -= 1
            depth = line.count("{") - line.count("}")
        pending_test_cfg = False
    if code > MAX_LINES:
        out.append(f"{code} non-test lines (max {MAX_LINES})")
    return out


def problems(path: pathlib.Path) -> list[str]:
    return problems_in(path.read_text(encoding="utf-8").splitlines())


# (case, source, expected to be flagged as an inline test module)
SELF_TEST = [
    ("a cfg(test) inline", "#[cfg(test)]\nmod tests {\n}", True),
    ("b no space before brace", "#[cfg(test)]\nmod tests{\n}", True),
    ("c pub(crate)", "#[cfg(test)]\npub(crate) mod tests {\n}", True),
    ("d attribute between", "#[cfg(test)]\n#[allow(dead_code)]\nmod tests {\n}", True),
    ("e cfg(all(test, unix))", "#[cfg(all(test, unix))]\nmod procfs {\n}", True),
    ("f cfg(any(test, feature))", '#[cfg(any(test, feature = "x"))]\nmod helpers {\n}', True),
    ("g comment and blank between", "#[cfg(test)]\n// why\n\nmod tests {\n}", True),
    ("h bare mod tests", "mod tests {\n    #[test]\n    fn t() {}\n}", True),
    ("i file module", "#[cfg(test)]\nmod tests;", False),
    ("j other inline module", "mod inner {\n}", False),
    ("k cfg(test) item, then a module", "#[cfg(test)]\nfn helper() {}\nmod inner {\n}", False),
    ("l a feature named testing", '#[cfg(feature = "testing")]\nmod inner {\n}', False),
]


def self_test() -> int:
    failed = 0
    for name, src, want in SELF_TEST:
        got = any("inline" in p for p in problems_in(src.split("\n")))
        if got != want:
            print(f"self-test {name}: flagged={got}, want {want}")
            failed += 1
    long = ["// x"] * MAX_LINES + ["#[cfg(test)]", "mod tests {"] + ["    // t"] * 50 + ["}"]
    if problems_in(long) != ["inline test module at line 302", f"{MAX_LINES + 1} non-test lines (max {MAX_LINES})"]:
        print(f"self-test line count: {problems_in(long)}")
        failed += 1
    print("self-test ok" if not failed else f"self-test: {failed} failed")
    return 1 if failed else 0


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
    sys.exit(self_test() if sys.argv[1:] == ["--self-test"] else main())
