"""Capture the inbox-format goldens in fixtures/inbox from the Python plugins and the TS mod.

    sh run.sh <work-dir> <herdr-inbox-src> <herdr-panorama-src> <fixtures/inbox>

(run.sh sets up the isolated environment, then runs this with the system python3 -I.)

The INPUTS below are chosen here: questions, answers, texts to parse, argv lists. Every
OUTPUT is what the shipped code does with them, read back from:

- herdr-inbox's Python (py_inbox.py): delivery text, answer format, header ids, rewake texts,
  ``inbox open`` parsing;
- herdr-inbox's MCP server over stdio (mcp_server.py): ``ask_operator`` results as the SDK
  serializes them;
- herdr-panorama's Python (py_panorama.py): receipt ids, ``inbox open`` arguments, the
  ``questions`` reader;
- the herdr-inbox-ui mod (ts_parse.mjs): parseAnswers / rewakeAnswers.

Parse inputs also include every text the mod's tests/captured.ts recorded from Claude Code.
"""

import json
import os
import sqlite3
import subprocess
import sys
from pathlib import Path

WORK, INBOX, PANO, OUT = (Path(p).resolve() for p in sys.argv[1:5])
HERE = Path(__file__).resolve().parent
PY_INBOX = WORK / "venv-inbox" / "bin" / "python"
PY_PANO = WORK / "venv-panorama" / "bin" / "python"
SESSION = "5f0c2a8e-3b1d-4c6e-9a7f-1e2d3c4b5a69"
TODAY = "Ship today?"
LONG = "a long answer " * 300  # 4200 chars: over herdr's 4000-char argument cap


def q(id, question, answer):
    return {"id": id, "question": question, "answer": answer}


ENCODE = [
    {"name": "single", "questions": [q(12, "Which DB?", "Postgres")]},
    {"name": "batch", "questions": [q(13, TODAY, "yes"), q(14, "Which region?", "eu-west-1")]},
    {"name": "batch-three", "questions": [q(1, "a?", "b"), q(2, "c?", "d"), q(3, "e?", "f")]},
    {"name": "multiline-question", "questions": [q(5, "Deploy now?\nDetails: the canary is green.", "go")]},
    {"name": "leading-blank-lines", "questions": [q(6, "\n\n   Title line   \nmore", "ok")]},
    {"name": "question-80", "questions": [q(7, "x" * 80, "ok")]},
    {"name": "question-81", "questions": [q(8, "y" * 81, "ok")]},
    {"name": "question-long-words", "questions": [q(9, "Should we " + "really " * 20 + "do it?", "no")]},
    {"name": "empty-question", "questions": [q(10, "", "ok")]},
    {"name": "blank-question", "questions": [q(11, " \n\t ", "ok")]},
    {"name": "crlf", "questions": [q(15, "First\r\nSecond", "ok")]},
    {"name": "line-separator", "questions": [q(16, "First\u2028Second", "ok")]},
    {"name": "next-line", "questions": [q(17, "First\x85Second", "ok")]},
    {"name": "file-separator", "questions": [q(18, "\x1cFirst\x1dSecond\x1e", "ok")]},
    {"name": "vertical-tab-form-feed", "questions": [q(19, "\x0bA\x0cB", "ok")]},
    {"name": "unicode-strip", "questions": [q(20, "\u3000\xa0Wide space\u2003", "ok")]},
    {"name": "emoji-at-cap", "questions": [q(21, "e" * 78 + "\U0001F680\U0001F680", "ok")]},
    {"name": "emoji-over-cap", "questions": [q(22, "e" * 79 + "\U0001F680\U0001F680", "ok")]},
    {"name": "combining", "questions": [q(23, "Cafe\u0301 " * 16, "oui")]},
    {"name": "answer-with-separators", "questions": [q(24, "Plan?", "a, b · Q99 see [inbox] Q98 answered — x: y")]},
    {"name": "answer-multiline", "questions": [q(25, "Notes?", "line one\nline two\n\nend")]},
    {"name": "answer-control", "questions": [q(26, "Which DB?", "Post\x1bgres\ttab")]},
    {"name": "answer-empty", "questions": [q(27, "Empty?", "")]},
    {"name": "big-id", "questions": [q(1234567, "Big?", "yes")]},
    {"name": "question-colon", "questions": [q(28, "Pick: A or B?", "A: the first")]},
    {"name": "long-answer", "questions": [q(29, "Long one?", LONG)]},
    {"name": "long-answer-in-batch", "questions": [q(30, "a?", "b"), q(31, "Long one?", LONG), q(32, "c?", "d")]},
    {"name": "split-real-cap", "questions": [q(40 + i, f"Part {i}?", "z" * 1500) for i in range(4)]},
    {"name": "small-cap-split", "max_arg": 60,
     "questions": [q(1, "a?", "b"), q(2, "c?", "d"), q(3, "e?", "f"), q(4, "g?", "h")]},
    {"name": "small-cap-reference", "max_arg": 60,
     "questions": [q(5, "Short?", "yes"), q(6, "Too long for the cap?", "x" * 40), q(7, "Last?", "no")]},
    {"name": "small-cap-exact", "max_arg": 41, "questions": [q(1, "a?", "b"), q(2, "c?", "d")]},
    {"name": "small-cap-exact-plus-one", "max_arg": 42, "questions": [q(1, "a?", "b"), q(2, "c?", "d")]},
    {"name": "small-cap-single-exact", "max_arg": 27, "questions": [q(1, "a?", "bcde")]},
    {"name": "small-cap-single-over", "max_arg": 26, "questions": [q(1, "a?", "bcde")]},
    {"name": "empty-list-messages", "questions": []},
]

FIRST_LINE = [
    {"text": t, "width": w} for t, w in [
        ("hello", 80), ("hello", 5), ("hello", 4), ("hello", 1), ("", 80), ("   ", 80),
        ("a\nb", 80), ("\n\nb\nc", 80), ("  pad  ", 80), ("x" * 200, 200), ("x" * 201, 200),
        ("tab\there", 80), ("\x1f\x1fsep", 80), ("a\x1cb", 80), ("a\u2029b", 80), ("a\rb", 80),
        ("\U0001F600" * 5, 3), ("\ufeffbom", 80), ("\u200bzero width", 80), ("\x85x", 80), ("hello", 0), ("", 0),
    ]
]

FORMAT_ANSWER = [
    {"choices": ["Postgres"]},
    {"choices": ["Postgres", "SQLite"]},
    {"choices": ["Postgres"], "notes": {"Postgres": "keep the old schema"}},
    {"choices": ["Postgres", "SQLite"], "notes": {"SQLite": "for tests"}, "text": "  and add a cache  "},
    {"choices": [], "text": "free text only"},
    {"choices": [], "text": "   "},
    {"choices": []},
    {"choices": ["a, b", 'say "hi"', "(x)", "dot · sep", "plain"]},
    {"choices": ["a"], "notes": {"a": ""}},
    {"choices": ["a"], "notes": {"b": "unused"}},
    {"choices": ['q"uote'], "notes": {'q"uote': "n"}},
]

PARSE_EXTRA = [
    "please mention [inbox] later",
    'why did you get "[inbox] Q12 answered — Which DB?: Postgres" twice?',
    "[inbox] Q12 answered —no space",
    "[inbox] Q12 answered — ",
    "[inbox] answered — ",
    "[inbox] answered — x · Q3 y: z",
    "[inbox] answered — Q1 a?: b · Q1 c?: d",
    "[inbox] answered — Q1 a · Q2 b",
    "[inbox] answered — Q1 · Q2 x: y",
    "[inbox] answered — Q1 a: see · Q99 notes · Q2 c: d",
    "[inbox] answered — Q1 a: b ·  Q2 c: d",
    "[inbox] answered —  Q1 a: b",
    "[inbox] answered — Q1a: b",
    "[inbox] Q007 answered — q: a",
    "[inbox] Q answered — q: a",
    "[inbox]  Q5 answered — q: a",
    "[inbox] q5 answered — q: a",
    "  \n[inbox] Q5 answered — q: a",
    "\ufeff[inbox] Q5 answered — q: a",
    "<pasted_content id=\"1\">[inbox] Q9 answered — q: a</pasted_content id=\"1\">",
    "<pasted_content id=\"1\">\n[inbox] answered — Q1 a: b · Q2 c: d\n</pasted_content id=\"1\">\n",
    "<pasted_content>\n[inbox] Q9 answered — q: a\n</pasted_content>",
    "x<pasted_content id=\"1\">\n[inbox] Q9 answered — q: a",
    "<pasted_content id=\"1\">\n<pasted_content id=\"2\">\n[inbox] Q9 answered — q: a",
    "[inbox] Q4 answered — Which DB?: Post\x1bgres",
    "[inbox] Q4 answered — Which DB?: Post\x00gres\x7f\x9f",
    "[inbox] Q4 answered — no colon here",
    "[inbox] Q4 answered — two: colons: here",
    "[inbox] Q4 answered — trailing:  spaced answer  \n",
    "[inbox] Q4 answered — multi\nline: answer\nmore",
    "some text typed first [inbox] Q4 answered — q: a",
    "<task-notification>\n<summary>Stop hook feedback</summary>\n</task-notification>\n<system-reminder>\n"
    "Stop hook blocking error from command \"Stop\": [inbox] answered — Q1 a?: b · Q2 c?: d\n\n</system-reminder>",
    "<task-notification>\n<summary>Stop hook feedback</summary>\n</task-notification>\n<system-reminder>\n"
    "Stop hook blocking error from command \"Stop\": [inbox] Q3 answered — a?: b\n\nlater [inbox] Q4 answered — c?: d"
    "</system-reminder>",
    "<task-notification>[inbox] Q5 answered — no end",
    "<task-notification>[inbox] Q5 answered — x: y   </task-notification> tail [inbox] Q6 answered — z: w \n \n rest",
    "<system-reminder>\nStop hook blocking error from command \"x\": [inbox] Q7 answered — q: a</system-reminder>",
    "<system-reminder>\nStop hook blocking error from command \"x\":[inbox] Q7 answered — q: a</system-reminder>",
    "[inbox] Q8 answered — q: a\n\u2028\nnext",
    "[inbox] Q8 answered — q: a\n\x1c\nnext",
    "[inbox] Q8 answered — q: a\r\n\r\nnext",
    "[inbox] answered — Q0 a · Q1 · Q2 y",
    "[inbox] answered — Q0 a: b · Q1 c: d</pasted_content a</pasted_content b>",
    "[inbox] Q9 answered — q: a </pasted_content> trailing",
    "[inbox] Q9 answered — q: a</pasted_content x>y>",
    "[inbox] Q9 answered — q: a\n</pasted_content id=\"1\">  \n",
    "<task-notification>[inbox] Q8 answered — q: a\n  indented\nmore",
    "<task-notification>[inbox] Q8 answered — q: a\n \t\nnext",
    "[inbox] answered — Q1 a · Q · Q5 b",
    "[inbox] answered — Q1 a · Q x · Q2 y",
    "<system-reminder>\nStop hook blocking error from command \"Stop\": [inbox] answered — Q1 a: b · Q2   </system-reminder>",
]

# question_ids inputs besides the MCP results: what else a tool result can hold
QUESTION_IDS_EXTRA = [
    '{"id": 6, "status": "opened"}',
    '{"id": 7, "status": "answered"}',
    '"id" : 8 ,\n "status":"already_open"',
    '"id": 9, "status": "open',
    "xQ5 Q5x _Q5 Q (Q7) Q8,Q9 Q10_ Q11é éQ12",
    "Recorded as Q13x and Recorded as Q14.",
    "Q99999999999999999999999 is too big",
]

OPEN_PARSE = [
    [], ["--question", "Q7"], ["--question", "q7"], ["--question", "7"], ["--question", "Q007"],
    ["--question", "laptop:Q12"], ["--question", "my-box.local:12"], ["--question", "Q"],
    ["--question", "Q7x"], ["--question", "QQ7"], ["--question", " Q7"], ["--question", "-1"],
    ["--question=Q8"], ["--group", "3"], ["--group", "G3"], ["--group", "03"], ["--pane", "w1:p2"],
    ["--pane", "has space"], ["--pane", ""], ["--pane", "w1:p2", "--no-hold"],
    ["--session", SESSION], ["--session", SESSION.upper()], ["--session", "not-a-uuid"],
    ["--session", SESSION + "x"], ["--question", "Q7", "--pane", "w1:p2"], ["--question", "Q7", "--no-hold"],
    ["--no-hold"], ["--no-hold", "--group", "4"], ["--question"], ["--bogus"], ["extra"],
    ["--question", "Q7", "--question", "Q8"],
    ["--question", "m" * 64 + ":Q1"], ["--question", "m" * 65 + ":Q1"], ["--question", ":Q1"],
    ["--question", "qnap:Q12"], ["--pane", "-.x"], ["--pane", "-"], ["--pane", "-1"],
    ["--pane", "a\x1cb"], ["--pane", "a\x85b"], ["--pane", "a\u3000b"],
    # herdr-inbox#55/#57: one optional Q, removed only right before the number
    ["--question", "qnap:q12"], ["--question", "qnap:12"], ["--question", "Qnap:Q7"], ["--question", "box:Q12"],
    ["--question", "box:12"], ["--question", "q12"], ["--question", "qq12"], ["--question", "QQ12"],
]

# question references (herdr-inbox#57, deeplink.question_ref / question_id / describe): the
# inputs of #57's own tests (tests/test_deep_link.py), plus box:Q12, two colons (the machine
# is split at the LAST one) and what Python's int() also reads after the Q
QUESTION_REF = [
    "qnap:Q12", "qnap:q12", "qnap:12", "Qnap:Q7", "box:Q12", "box:12", "Q12", "q12", "12", "QQ12", "qq12",
    "", "nap", "Q", "a:b:Q12", "a:Q1:Q2", "Q 12", "Q+12", "Q1_2", "Q-0", "Q-1", "Q007",
]

OPEN_ARGS = [
    {"question": 7, "version": "0.10.6", "help": "usage: inbox open [--no-hold]"},
    {"question": 7, "version": "0.10.6", "help": "usage: inbox open"},
    {"question": 0, "version": "0.10.6", "help": "usage: inbox open [--no-hold]"},
    {"pane": "w1:p2", "version": "0.10.6", "help": "usage: inbox open [--no-hold]"},
    {"pane": "w1:p2", "version": "0.10.6", "help": None},
    {"pane": "w1:p2\nnext\x1b", "version": "0.10.6", "help": "--no-hold"},
    {"question": 7, "pane": "w1:p2", "version": "0.10.6", "help": "--no-hold"},
    {"version": "0.10.6", "help": "--no-hold"},
    {"pane": "", "version": "0.10.6", "help": "--no-hold"},
    # deep-link support by version (>= 0.6.0), else by `inbox open --help` naming --pane
    {"question": 7, "version": "0.6.0", "help": None},
    {"question": 7, "version": "0.6", "help": None},
    {"question": 7, "version": "0.5.9", "help": "usage: inbox open [--pane PANE_ID]"},
    {"question": 7, "version": "0.5.9", "help": "usage: inbox open"},
    {"question": 7, "version": "0.5.9", "help": None},
    {"pane": "w1:p2", "version": "1.0.0-rc1", "help": None},
    {"pane": "w1:p2", "version": "v1.0.0", "help": None},
    {"pane": "w1:p2", "version": "", "help": "--pane --no-hold"},
    {"pane": "w1:p2", "version": "10.0", "help": None},
    {"pane": "w1:p2", "version": "0.10", "help": None},
]


def run(argv, **kw):
    return subprocess.run([str(a) for a in argv], check=True, **kw)


def mcp_session(env_extra, calls):
    env = {**os.environ, **env_extra}
    p = subprocess.Popen([str(PY_INBOX), "-I", str(HERE / "mcp_server.py")], stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE, text=True, env=env, cwd=str(WORK))

    def rpc(id, method, params=None):
        msg = {"jsonrpc": "2.0", "id": id, "method": method}
        if params is not None:
            msg["params"] = params
        p.stdin.write(json.dumps(msg) + "\n")
        p.stdin.flush()
        return json.loads(p.stdout.readline())

    rpc(0, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                          "clientInfo": {"name": "capture", "version": "0"}})
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
    p.stdin.flush()
    out = []
    for i, (name, args, before) in enumerate(calls, start=1):
        if before:
            before()
        reply = rpc(i, "tools/call", {"name": "ask_operator", "arguments": args})
        out.append({"name": name, "arguments": args, "result": reply["result"]})
    p.stdin.close()
    p.wait(timeout=30)
    return out


def answer_q1():
    code = ("from herdr_inbox import service; from herdr_inbox.store import Store\n"
            "with Store() as s:\n    assert service.answer(s, 1, 'Postgres', deliver=False)")
    run([PY_INBOX, "-I", "-c", code], cwd=str(WORK))


def receipts():
    db = WORK / "state" / "inbox.db"
    run([PY_INBOX, "-I", "-c", "from herdr_inbox.store import Store\nStore(migrate=True).db.close()"], cwd=str(WORK))
    which = "Which database should the service use?"
    in_herdr = mcp_session({"CAPTURE_PANE": "w1:p1", "CAPTURE_SESSION": SESSION}, [
        ("open", {"question": which, "options": ["Postgres", "SQLite"], "asked_by": "capture"}, None),
        ("already-open", {"question": which, "options": ["Postgres", "SQLite"]}, None),
        ("group-open", {"title": "Release", "questions": [
            {"question": TODAY}, {"question": "Which region?", "header": "Region",
                                  "options": [{"label": "eu-west-1", "description": "Ireland"}, "us-east-1"]}]}, None),
        ("group-mixed", {"questions": [{"question": which}, {"question": "Run the migration now?"}]}, None),
        ("answered", {"question": which}, answer_q1),
        ("error-both", {"question": "x?", "questions": [{"question": "y?"}]}, None),
        ("error-neither", {}, None),
        ("error-options", {"question": "Too many?", "options": [str(i) for i in range(30)]}, None),
    ])
    outside = mcp_session({}, [("unavailable", {"question": "Anyone there?"}, None)])
    return in_herdr + outside, db


def v01_db():
    path = WORK / "state" / "v01.db"
    code = ("import sys; sys.path.insert(0, sys.argv[1]); import compat_v01_store as v\n"
            "s = v.Store(sys.argv[2])\n"
            "a = s.insert('Old question one?', pane_id='w1:p1', session_id=sys.argv[3], asked_by='old')\n"
            "b = s.insert('Old answered?', pane_id='w1:p1', session_id=sys.argv[3])\n"
            "s.set_answer(b.id, 'yes')\n"
            "c = s.insert('No pane, no session', options=['a', 'b'])\n"
            "s.close()")
    run([PY_INBOX, "-I", "-c", code, INBOX / "tests", path, SESSION], cwd=str(WORK))
    return path


def dump(path, name):
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    text = "\n".join(con.iterdump()) + "\n"
    con.close()
    (OUT / "questions-db").mkdir(exist_ok=True)
    (OUT / "questions-db" / f"{name}.sql").write_text(text.replace(str(WORK), "<work>"))


def main():
    work_cases = WORK / "cases"
    work_cases.mkdir(exist_ok=True)
    captured_path = work_cases / "captured.json"
    run(["node", HERE / "ts_parse.mjs", "captured", INBOX / "plugins/herdr-inbox-ui/tests/captured.ts", captured_path])
    captured = json.loads(captured_path.read_text())
    captured_texts = []

    def walk(v):
        if isinstance(v, dict):
            for k, x in v.items():
                if k in ("text", "content") and isinstance(x, str) and "[inbox]" in x:
                    captured_texts.append(x)
                else:
                    walk(x)
        elif isinstance(v, list):
            for x in v:
                if isinstance(x, str) and "[inbox]" in x:
                    captured_texts.append(x)
                else:
                    walk(x)

    walk(captured)
    calls, db = receipts()
    v01 = v01_db()
    empty = WORK / "state" / "no-table.db"
    sqlite3.connect(empty).execute("CREATE TABLE other (x)").connection.close()

    py_in = work_cases / "inbox-in.json"
    py_in.write_text(json.dumps({"encode": ENCODE, "first_line": FIRST_LINE, "format_answer": FORMAT_ANSWER,
                                 "parse": [], "open_parse": OPEN_PARSE,
                                 "question_ref": QUESTION_REF}))
    run([PY_INBOX, "-I", HERE / "py_inbox.py", py_in, work_cases / "inbox-enc.json"], cwd=str(WORK))
    enc = json.loads((work_cases / "inbox-enc.json").read_text())
    produced = []
    for c in enc["encode"]:
        produced += [c["delivery_text"], *c["references"], *(m["text"] for m in c["messages"])]
    parse_inputs = list(dict.fromkeys(captured_texts + produced + PARSE_EXTRA))
    py_in.write_text(json.dumps({"encode": [], "first_line": [], "format_answer": [], "parse": parse_inputs,
                                 "open_parse": [], "question_ref": []}))
    run([PY_INBOX, "-I", HERE / "py_inbox.py", py_in, work_cases / "inbox-parse.json"], cwd=str(WORK))
    py_parse = json.loads((work_cases / "inbox-parse.json").read_text())["parse"]
    (work_cases / "ts-in.json").write_text(json.dumps(parse_inputs))
    run(["node", HERE / "ts_parse.mjs", "parse", INBOX / "plugins/herdr-inbox-ui/hooks/register.tsx",
         work_cases / "ts-in.json", work_cases / "ts-parse.json"])
    ts_parse = json.loads((work_cases / "ts-parse.json").read_text())

    texts = [c["result"]["content"][0]["text"] for c in calls]
    # Two writers holding the write lock: in WAL mode (the inbox's own) readers are not
    # blocked; in rollback-journal mode they wait for the reader's timeout, then give up.
    holders = []
    for name, mode in (("v01-wal-writer-busy", "WAL"), ("v01-rollback-locked", "DELETE")):
        copy = WORK / "state" / f"{name}.db"
        copy.write_bytes(v01.read_bytes())
        holder = sqlite3.connect(copy, isolation_level=None)
        assert holder.execute(f"PRAGMA journal_mode={mode}").fetchone()[0].upper() == mode
        holder.execute("BEGIN EXCLUSIVE")
        holder.execute("UPDATE questions SET repeat_count = repeat_count + 1")
        holders.append((name, copy, holder))
    pano_in = work_cases / "panorama-in.json"
    pano_in.write_text(json.dumps({"question_ids": texts + QUESTION_IDS_EXTRA, "open_args": OPEN_ARGS, "open_questions": [
        ["current", str(db)], ["v01", str(v01)], ["no-table", str(empty)], ["missing", str(WORK / "state" / "nope.db")],
        ["directory", str(WORK / "state")], *([n, str(c)] for n, c, _ in holders)]}))
    run([PY_PANO, "-I", HERE / "py_panorama.py", pano_in, work_cases / "panorama-out.json"], cwd=str(WORK))
    for _, _, holder in holders:
        holder.execute("ROLLBACK")
        holder.close()
    pano = json.loads((work_cases / "panorama-out.json").read_text())

    meta = json.loads((WORK / "meta.json").read_text())
    for c, r in zip(calls, pano["question_ids"]):
        c["text"], c["question_ids"] = r["text"], r["ids"]
    parse = [{**p, "answers": t["answers"], "rewake_answers": t["rewake_answers"],
              "from_captured_ts": p["text"] in captured_texts}
             for p, t in zip(py_parse, ts_parse, strict=True)]
    write("delivery.json", {"source": meta, "max_arg": enc["max_arg"], "encode": enc["encode"],
                            "first_line": enc["first_line"], "format_answer": enc["format_answer"], "parse": parse})
    extra = pano["question_ids"][len(calls):]
    write("receipts.json", {"source": meta, "constants": enc["receipt_constants"], "calls": calls,
                            "question_ids_extra": extra})
    write("open-args.json", {"source": meta, "encode": pano["open_args"], "parse": enc["open_parse"],
                             "question_ref": enc["question_ref"]})
    dump(db, "current")
    dump(v01, "v01")
    dump(empty, "no-table")
    write("questions-db/expected.json", {"source": meta, "reads": pano["open_questions"]})


def write(name, data):
    (OUT / name).parent.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n")


main()
