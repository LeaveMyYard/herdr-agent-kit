"""Run herdr-panorama's own readers of the inbox's formats (run in panorama's venv).

    python py_panorama.py <cases.json> <out.json>

- ``scan.wire.question_ids``: the Q ids panorama reads out of an ``ask_operator`` result.
- ``inbox_link.open_inbox``: the ``inbox open`` arguments panorama passes. Its herdr lookups
  are replaced by fixed answers (the inbox's plugin info, its ``open --help`` and the command
  run itself, which is recorded instead of run); the argument building is panorama's.
- ``inbox.open_questions``: panorama's read-only reader of the inbox database.

Run only from capture.py (isolated env, failing herdr stub).
"""

import json
import os
import sys
from pathlib import Path

from herdr_panorama import herdr, inbox, inbox_link
from herdr_panorama.scan import wire


def open_args(case):
    calls = []
    inbox_link._cache.clear()
    inbox_link.plugin_info = lambda: {"plugin_root": "/inbox", "version": case["version"]}
    inbox_link._help = lambda info: case["help"]

    def run(root, *args, timeout=20.0):
        calls.append({"args": list(args), "argv": [herdr.clean(a) for a in args]})
        return 0, ""

    inbox_link._inbox = run
    herdr.invoke_action = lambda action: calls.append({"action": action})
    inbox_link.open_inbox(question=case.get("question"), pane=case.get("pane"))
    return calls


def main():
    assert not any(k.startswith("HERDR_") and k not in ("HERDR_INBOX_DB", "HERDR_INBOX_NO_WORKER")
                   for k in os.environ), "run from capture.py: no HERDR_* but the capture's own"
    cases = json.load(open(sys.argv[1]))
    out = {
        "question_ids": [{"text": t, "ids": sorted(wire.question_ids(t))} for t in cases["question_ids"]],
        "open_args": [{**c, "calls": open_args(c)} for c in cases["open_args"]],
        "open_questions": [],
    }
    for name, path in cases["open_questions"]:
        qs = inbox.open_questions(Path(path))
        out["open_questions"].append({"db": name, "questions": None if qs is None else [q.__dict__ for q in qs]})
    json.dump(out, open(sys.argv[2], "w"), ensure_ascii=False, indent=1)


main()
