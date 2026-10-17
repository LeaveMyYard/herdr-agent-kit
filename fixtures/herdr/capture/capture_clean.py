"""Capture `clean()` goldens from the two Python herdr clients.

Usage (from the kit root, with both plugin repos fetched):

    git -C ~/Code/herdr-panorama show origin/main:src/herdr_panorama/herdr.py > /path/panorama_herdr.py
    git -C ~/Code/herdr-inbox show origin/main:src/herdr_inbox/herdr.py > /path/inbox_herdr.py
    python3 -I fixtures/herdr/capture/capture_clean.py /path/panorama_herdr.py /path/inbox_herdr.py \
        > fixtures/herdr/clean/clean.jsonl

Only the `MAX_ARG`, `_SPACE`, `_CONTROL` and `clean` definitions are taken from each file
(with `ast`, so the modules' own imports never run). Both plugins must give the same output
for every input; the script stops if they differ. Each output line is
`{"text", "limit", "out"}`; `limit` is null for the default (`MAX_ARG`).
"""

import ast
import json
import re
import sys

WANTED = {"MAX_ARG", "_SPACE", "_CONTROL", "clean"}


def load(path):
    tree = ast.parse(open(path, encoding="utf-8").read())
    keep = []
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name in WANTED:
            keep.append(node)
        elif isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id in WANTED for t in node.targets):
            keep.append(node)
    names = {getattr(n, "name", None) or n.targets[0].id for n in keep}
    assert names == WANTED, (path, names)
    ns = {"re": re}
    exec(compile(ast.Module(body=keep, type_ignores=[]), path, "exec"), ns)
    return ns


def corpus():
    for cp in list(range(0x00, 0xA1)) + [0xAD, 0x2028, 0x2029, 0x200B, 0x200E, 0xFEFF, 0x1F600, 0xE9, 0x301]:
        yield "a" + chr(cp) + "b", None
    yield "", None
    yield "plain text", None
    yield "a\x1b[31mb\tc\nd\x07", None  # panorama tests/test_herdr.py
    yield "\x1b]0;title\x07rest", None
    yield "\x1b[2J\x1b[H", None
    yield "line1\r\nline2 line3 line4\x85end", None
    yield "w1:p1\x1b[2J", None
    yield "tab\there  two\x0bvt\x0cff", None
    yield "\x00\x01\x7f\x80\x9f\xa0", None
    yield "café ❓ ◆ \U0001F600 …", None
    for n in (3999, 4000, 4001, 5000):
        yield "x" * n, None
    yield "é" * 4001, None
    yield "\U0001F600" * 4001, None
    yield "\x1b" * 10 + "y" * 4000, None
    yield "\n" * 4001, None
    for limit in (0, 1, 2, 3, 10, 200, 300):
        yield "abcdefghijklmnop" * 20, limit
        yield "ab", limit
        yield "\x1b[31mcolour\x1b[0m and more text here", limit
    yield "exactly ten", 10
    yield "exactly 10", 10
    yield "a\nb", 10 ** 9  # herdr-inbox hook.py's limit


def main():
    pano, inbox = (load(p) for p in sys.argv[1:3])
    for text, limit in corpus():
        args = (text,) if limit is None else (text, limit)
        a, b = pano["clean"](*args), inbox["clean"](*args)
        if a != b:
            sys.exit(f"panorama and inbox differ on {text!r} (limit {limit}): {a!r} != {b!r}")
        print(json.dumps({"text": text, "limit": limit, "out": a}))
    print(json.dumps({"max_arg": pano["MAX_ARG"], "inbox_max_arg": inbox["MAX_ARG"]}), file=sys.stderr)


if __name__ == "__main__":
    main()
