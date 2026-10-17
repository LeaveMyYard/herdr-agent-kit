"""Run herdr-inbox's own format code over the capture's inputs (run in the inbox's venv).

    python py_inbox.py <cases.json> <out.json>

Every output value comes from the inbox's functions on the exported source: service.py
(delivery text, answer format), transcript.py (header ids, rewake texts) and cli.py plus
deeplink.py (``inbox open``: the request file it writes, or its argparse error). Nothing
here re-implements a format. Run only from run.sh (isolated env, failing herdr stub).
"""

import contextlib
import io
import json
import os
import sys
from types import SimpleNamespace

from herdr_inbox import cli, deeplink, herdr, service, transcript


def question(q):
    # delivery_text and its helpers read only .ref, .question and .answer of a Question
    return SimpleNamespace(id=q["id"], ref=f"Q{q['id']}", question=q["question"], answer=q["answer"])


def encode(case):
    qs = [question(q) for q in case["questions"]]
    saved = herdr.MAX_ARG
    if "max_arg" in case:
        herdr.MAX_ARG = case["max_arg"]  # service reads herdr.MAX_ARG at call time
    try:
        return {
            "max_arg": herdr.MAX_ARG,
            "delivery_text": service.delivery_text(qs),
            "answer_lines": [service.answer_line(q) for q in qs],
            "references": [service._reference(q) for q in qs],
            "messages": [{"text": text, "ids": [q.id for q in batch], "acknowledged": ack}
                         for text, batch, ack in service.delivery_messages(qs)],
        }
    finally:
        herdr.MAX_ARG = saved


def open_parse(argv):
    req = deeplink.request_path()
    with contextlib.suppress(FileNotFoundError):
        req.unlink()
    err = io.StringIO()
    try:
        with contextlib.redirect_stderr(err):
            code = cli.main(["open", *argv])
    except SystemExit as exc:
        code = exc.code
    out = {"exit": code}
    if code == 2:  # argparse's error: its last line (the usage lines above it list every command)
        out["error"] = err.getvalue().strip().splitlines()[-1]
    if req.exists():
        data = json.loads(req.read_text())
        out["request"] = {k: data[k] for k in ("kind", "value", "hold") if k in data}
        req.unlink()
    return out


def main():
    assert not any(k.startswith("HERDR_") and k not in ("HERDR_INBOX_DB", "HERDR_INBOX_NO_WORKER")
                   for k in os.environ), "run from run.sh: no HERDR_* but the capture's own"
    cases = json.load(open(sys.argv[1]))
    out = {
        "max_arg": herdr.MAX_ARG,
        "encode": [{**c, **encode(c)} for c in cases["encode"]],
        "first_line": [{**c, "out": service.first_line(c["text"], c["width"])} for c in cases["first_line"]],
        "format_answer": [{**c, "out": service.format_answer(c["choices"], c.get("notes"), c.get("text"))}
                          for c in cases["format_answer"]],
        "parse": [{"text": t, "header_ids": sorted(transcript.header_ids(t)),
                   "rewake_texts": list(transcript._rewake_texts(t))} for t in cases["parse"]],
        "open_parse": [{"argv": a, **open_parse(a)} for a in cases["open_parse"]],
        "receipt_constants": {"unavailable": service.UNAVAILABLE, "not_migrated": service.NOT_MIGRATED},
    }
    json.dump(out, open(sys.argv[2], "w"), ensure_ascii=False, indent=1)


main()
