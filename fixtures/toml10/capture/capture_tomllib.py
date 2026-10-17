"""Capture what Python's tomllib (TOML 1.0) makes of each input: the goldens in ../cases.json.

Usage: python3 -I capture_tomllib.py TOML_TEST_CHECKOUT > ../cases.json

TOML_TEST_CHECKOUT is a checkout of https://github.com/toml-lang/toml-test (MIT); every
`tests/valid/**/*.toml` and `tests/invalid/**/*.toml` file in it becomes a case, after the
hand-written CASES below (the inputs where TOML 1.0 and 1.1 differ, and implementation
limits the two plugins' parsers may not share).

Each case records the input (`toml`, or `toml_hex` when it is not UTF-8) and what tomllib
did with it, the way both Python plugins call it (`tomllib.load` on the file's bytes):

- `{"ok": <value>}`: the document, tagged as below.
- `{"error": "<message>"}`: tomllib raised TOMLDecodeError. Both plugins log
  "ignoring <path>: <message>" and use an empty table.
- `{"raises": "<exception class>"}`: tomllib raised something else (not caught by the
  plugins' `except (OSError, tomllib.TOMLDecodeError)`).

Value tags: a table is a JSON object, an array a JSON array, and a scalar is
`{"type": T, "value": V}` with T one of string, integer (V in decimal), float (V the
IEEE-754 bits as 16 hex digits, or "nan" for any NaN), bool, datetime, datetime-local,
date-local, time-local. Date-times are ISO 8601 with exactly six fraction digits
(tomllib keeps microseconds, truncating) and the offset as +HH:MM (Z is +00:00).
"""

from __future__ import annotations

import datetime
import json
import math
import pathlib
import platform
import struct
import subprocess
import sys
import tomllib

# (name, input). Inputs are bytes so non-UTF-8 and BOM cases can be written.
CASES: list[tuple[str, bytes]] = [
    # --- TOML 1.1 features that 1.0 (tomllib) rejects ---
    ("inline-trailing-comma", b'x = { a = 1, }\n'),
    ("inline-trailing-comma-nested", b'x = { a = { b = 1, } }\n'),
    ("inline-only-comma", b'x = {,}\n'),
    ("inline-newline-after-open", b'x = {\na = 1 }\n'),
    ("inline-newline-before-close", b'x = { a = 1\n}\n'),
    ("inline-newline-between", b'x = { a = 1,\nb = 2 }\n'),
    ("inline-empty-newline", b'x = {\n}\n'),
    ("inline-comment", b'x = { a = 1, # c\nb = 2 }\n'),
    ("inline-crlf", b'x = { a = 1,\r\nb = 2 }\n'),
    ("inline-in-array-newline-inside", b'x = [ { a = 1,\n} ]\n'),
    ("escape-e", b'x = "\\e"\n'),
    ("escape-x", b'x = "\\x41"\n'),
    ("escape-x-high", b'x = "\\xE9"\n'),
    ("escape-e-multiline", b'x = """\\e"""\n'),
    ("escape-x-multiline", b'x = """\n\\x41"""\n'),
    ("escape-e-key", b'"\\e" = 1\n'),
    ("escape-x-key", b'"\\x41" = 1\n'),
    ("escape-e-table-key", b'["\\e"]\na = 1\n'),
    ("escape-e-inline-key", b'x = { "\\x41" = 1 }\n'),
    ("time-no-seconds", b'x = 07:32\n'),
    ("datetime-local-no-seconds", b'x = 1979-05-27T07:32\n'),
    ("datetime-local-no-seconds-space", b'x = 1979-05-27 07:32\n'),
    ("datetime-no-seconds-z", b'x = 1979-05-27T07:32Z\n'),
    ("datetime-no-seconds-offset", b'x = 1979-05-27T07:32-07:00\n'),
    ("datetime-no-seconds-in-array", b'x = [ 07:32 ]\n'),
    ("datetime-no-seconds-in-inline", b'x = { t = 07:32 }\n'),
    # --- the same shapes in TOML 1.0, which both accept ---
    ("inline-plain", b'x = { a = 1, b = { c = "d" } }\n'),
    ("inline-empty", b'x = {}\n'),
    ("inline-array-with-newlines", b'x = { a = [\n1,\n2,\n] }\n'),
    ("inline-array-with-comment", b'x = { a = [ 1, # c\n2 ] }\n'),
    ("inline-multiline-string", b'x = { a = """\nmulti\nline""" }\n'),
    ("inline-multiline-literal", b"x = { a = '''\nmulti''' }\n"),
    ("array-trailing-comma", b'x = [ 1, 2, ]\n'),
    ("array-of-inline-newlines-between", b'x = [\n{ a = 1 },\n{ b = 2 },\n]\n'),
    ("escape-backslash-e", b'x = "\\\\e \\\\x41"\n'),
    ("escape-u", b'x = "\\u00E9 \\U0001F600"\n'),
    ("escape-line-ending-backslash", b'x = """a \\\n   e"""\n'),
    ("literal-backslash-e-x", b"x = '\\e\\x41'\n"),
    ("literal-multiline-backslash-e", b"x = '''\\e'''\n"),
    ("literal-key-backslash-e", b"'\\e' = 1\n"),
    ("comment-backslash-e", b'x = 1 # \\e \\x41 { a = 1, }\n'),
    ("time-with-seconds", b'x = 07:32:00\n'),
    ("time-fraction", b'x = 07:32:00.999999999\n'),
    ("datetime-local-with-seconds", b'x = 1979-05-27T07:32:00\n'),
    ("datetime-space", b'x = 1979-05-27 07:32:00Z\n'),
    ("datetime-lowercase", b'x = 1979-05-27t07:32:00z\n'),
    ("datetime-offset", b'x = 1979-05-27T00:32:00.5-07:00\n'),
    ("date-local", b'x = 1979-05-27\n'),
    ("string-looks-like-time", b'x = "07:32"\n'),
    # --- implementation limits and edges ---
    ("bom", b'\xef\xbb\xbfx = 1\n'),
    ("not-utf8", b'x = "\xff"\n'),
    ("int-max", b'x = 9223372036854775807\n'),
    ("int-min", b'x = -9223372036854775808\n'),
    ("int-over-max", b'x = 9223372036854775808\n'),
    ("int-under-min", b'x = -9223372036854775809\n'),
    ("hex-over-max", b'x = 0xffffffffffffffff\n'),
    ("float-overflow", b'x = 1e400\n'),
    ("float-underflow", b'x = 1e-400\n'),
    ("float-neg-zero", b'x = -0.0\n'),
    ("float-nan-inf", b'a = nan\nb = -nan\nc = +inf\nd = -inf\n'),
    ("float-precision", b'x = 0.1\ny = 3.141592653589793238\n'),
    ("leap-second", b'x = 1990-12-31T23:59:60Z\n'),
    ("leap-second-local-time", b'x = 23:59:60\n'),
    ("leap-second-fraction", b'x = 1990-12-31 23:59:60.5\n'),
    ("leap-second-local-datetime", b'x = [ 1990-12-31T23:59:60 ]\n'),
    ("second-59", b'x = 23:59:59.999999\n'),
    ("hour-24", b'x = 24:00:00\n'),
    ("minute-60", b'x = 23:60:00\n'),
    ("month-13", b'x = 2024-13-01\n'),
    ("april-31", b'x = 2024-04-31\n'),
    ("year-zero-datetime", b'x = 0000-01-01T00:00:00Z\n'),
    ("year-one", b'x = 0001-01-01\n'),
    ("year-9999", b'x = 9999-12-31T23:59:59-23:59\n'),
    ("offset-minute-60", b'x = 1979-05-27T07:32:00+05:60\n'),
    ("feb-29-non-leap", b'x = 2023-02-29\n'),
    ("feb-29-leap", b'x = 2024-02-29\n'),
    ("year-zero", b'x = 0000-01-01\n'),
    ("offset-24h", b'x = 1979-05-27T07:32:00+24:00\n'),
    ("offset-23-59", b'x = 1979-05-27T07:32:00+23:59\n'),
    ("offset-minus-zero", b'x = 1979-05-27T07:32:00-00:00\n'),
    ("surrogate-escape", b'x = "\\uD800"\n'),
    ("escape-over-max", b'x = "\\U00110000"\n'),
    ("del-in-comment", b'x = 1 # a\x7fb\n'),
    ("del-in-string", b'x = "a\x7fb"\n'),
    ("lone-cr", b'x = 1\ry = 2\n'),
    ("crlf-in-multiline", b'x = """a\r\nb"""\n'),
    ("crlf-in-multiline-literal", b"x = '''a\r\nb'''\r\ny = 1\r\n"),
    ("crlf-line-ending-backslash", b'x = """a \\\r\n  b"""\n'),
    ("cr-in-multiline", b'x = """a\rb"""\n'),
    ("crlf-position", b'a = 1\r\nb = { c = 1, }\r\n'),
    ("unicode-bare-key", 'é = 1\n'.encode()),
    ("deep-array", b'x = ' + b'[' * 100 + b']' * 100 + b'\n'),
    ("deep-inline", b'x = ' + b'{ a = ' * 100 + b'1' + b' }' * 100 + b'\n'),
    # --- what the plugins' config.toml files look like ---
    (
        "panorama-config",
        b'done_window = "1h"\nstale_after = "12h"\ntint = 0.15\nhome_tab = false\n'
        b'tab_prefix = "\xe2\x9d\x93"\n[colors]\nidle = "#888888"\n',
    ),
    ("inbox-config", b'tab_prefix = "\xe2\x9d\x93 "\nprefix_unnamed = true\n'),
]


def tag(v: object) -> object:
    if isinstance(v, dict):
        return {k: tag(x) for k, x in v.items()}
    if isinstance(v, list):
        return [tag(x) for x in v]
    if isinstance(v, bool):
        return {"type": "bool", "value": "true" if v else "false"}
    if isinstance(v, int):
        return {"type": "integer", "value": str(v)}
    if isinstance(v, float):
        bits = "nan" if math.isnan(v) else struct.pack(">d", v).hex()
        return {"type": "float", "value": bits}
    if isinstance(v, str):
        return {"type": "string", "value": v}
    if isinstance(v, datetime.datetime):
        if v.tzinfo is None:
            return {"type": "datetime-local", "value": v.isoformat(timespec="microseconds")}
        return {"type": "datetime", "value": v.isoformat(timespec="microseconds")}
    if isinstance(v, datetime.date):
        return {"type": "date-local", "value": v.isoformat()}
    if isinstance(v, datetime.time):
        return {"type": "time-local", "value": v.isoformat(timespec="microseconds")}
    raise TypeError(type(v))


def capture(name: str, raw: bytes) -> dict:
    case: dict = {"name": name}
    try:
        case["toml"] = raw.decode("utf-8")
    except UnicodeDecodeError:
        case["toml_hex"] = raw.hex()
    try:
        # What tomllib.load does with the file's bytes.
        doc = tomllib.loads(raw.decode())
    except tomllib.TOMLDecodeError as exc:
        case["python"] = {"error": str(exc)}
    except Exception as exc:  # noqa: BLE001 - recorded, not handled
        case["python"] = {"raises": type(exc).__name__}
    else:
        case["python"] = {"ok": tag(doc)}
    return case


def main() -> None:
    corpus = pathlib.Path(sys.argv[1])
    rev = subprocess.run(
        ["git", "-C", str(corpus), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()
    cases = [capture(name, raw) for name, raw in CASES]
    for path in sorted(corpus.glob("tests/*/**/*.toml")):
        rel = path.relative_to(corpus / "tests")
        if rel.parts[0] in ("valid", "invalid"):
            cases.append(capture(f"toml-test/{rel.as_posix()}", path.read_bytes()))
    out = {
        "captured_with": {
            "python": platform.python_version(),
            "implementation": platform.python_implementation(),
            "module": "tomllib",
            "toml_test_commit": rev,
        },
        "cases": cases,
    }
    # One case per line, so a re-capture diffs case by case.
    sys.stdout.write('{"captured_with": ' + json.dumps(out["captured_with"]) + ',\n "cases": [\n')
    sys.stdout.write(",\n".join("  " + json.dumps(c, ensure_ascii=False) for c in cases))
    sys.stdout.write("\n]}\n")


if __name__ == "__main__":
    main()
