# inbox-format goldens

The canonical goldens of herdr-inbox's published formats, read by `inbox_contract`'s tests
(`tests/inbox_contract_goldens.rs`). herdr-inbox vendors a copy of `delivery.json` for its
`herdr-inbox-ui` mod's tests and checks it against the pinned kit tag.

Captured 2026-10-17 by `capture/run.sh` from clean `git archive` exports of herdr-inbox
`8771a493a2e1a299f60b36813f141a375159266f` and herdr-panorama
`83590dccbf478ed7927a450a3198ffae54ef88f5` (both `main`, after herdr-inbox#57 fixed question
references, #55), with node v24.12.0. Each file's `source` repeats this. Nothing was edited by hand.

**The inputs are chosen, the outputs are captured.** `capture/capture.py` lists every input
(questions, answers, texts to parse, argument lists); every expected value is what the
shipped code returned for it. The parse inputs also include every text the mod's
`tests/captured.ts` recorded from Claude Code 2.1.301 (`"from_captured_ts": true`).

| file | what | written by | read by |
|---|---|---|---|
| `delivery.json` `encode` | `[inbox] …` messages: `delivery_text`, `answer_line`, `_reference`, `delivery_messages` (some cases with a smaller `max_arg`) | inbox `service.py` | |
| `delivery.json` `first_line`, `format_answer` | the question's first line, the answer's one-line form | inbox `service.py` | |
| `delivery.json` `parse` | per text: `header_ids`, `rewake_texts` | | inbox `transcript.py` |
| | per text: `answers`, `rewake_answers` | | mod `register.tsx` (`parseAnswers`, `rewakeAnswers`) |
| `receipts.json` `calls` | `ask_operator` results (`result` is the JSON-RPC result, `text` its text content) and the ids in them | inbox MCP server | panorama `scan/wire.py` (`question_ids`) |
| `receipts.json` `question_ids_extra`, `constants` | more `question_ids` inputs; `service.UNAVAILABLE` / `NOT_MIGRATED` | inbox `service.py` | panorama `scan/wire.py` |
| `open-args.json` `encode` | the `inbox open` arguments panorama passes (`args` raw, `argv` after panorama's `herdr.clean`), or the plain `open` action | | panorama `inbox_link.py` |
| `open-args.json` `parse` | per argument list: the focus request `inbox open` wrote, or its argparse error (exit 2) | | inbox `cli.py`, `deeplink.py` |
| `open-args.json` `question_ref` | per reference: `question_ref`, `question_id` (`null`: refused) and `describe` | inbox `deeplink.py` | inbox `cli.py`, `deeplink.py` |
| `questions-db/*.sql` | databases made by the inbox's code: `current` (the receipts session's), `v01` (0.1's `store.py`, kept in inbox `tests/compat_v01_store.py`), `no-table` | inbox | |
| `questions-db/expected.json` | panorama's reader on each, plus a missing file, a directory and the 0.1 database while a writer holds its lock (WAL: reads; rollback journal: "no inbox") | | panorama `inbox.py` |

## How it was captured

- `capture/run.sh` makes a venv per plugin from the exports and runs `capture/capture.py`
  under `env -i`: `HOME`, `XDG_STATE_HOME` and `XDG_CONFIG_HOME` in the work dir, no
  `HERDR_*` apart from `HERDR_INBOX_DB` (work dir) and `HERDR_INBOX_NO_WORKER=1`, and a
  `herdr` stub that always fails first in `PATH`. No real herdr, Claude Code or credential
  was used; the operator's inbox database was not opened.
- `py_inbox.py` and `py_panorama.py` call the plugins' functions on the inputs.
  `inbox open` runs as `cli.main` (its request file is read back; opening the popup then
  fails at the herdr stub). panorama's herdr lookups in `inbox_link` (plugin info,
  `inbox open --help`, running the command) are replaced by fixed answers; the argument
  building is its own.
- `mcp_server.py` is the inbox's MCP server with one test double: `resolve_caller` returns
  a fixed caller (pane `w1:p1`, a fixed session id; none for the "unavailable" call),
  because finding the caller needs herdr. `capture.py` speaks MCP stdio to it.
- `ts_parse.mjs` cuts `clean`, `parseAnswers` and `rewakeAnswers` verbatim out of
  `register.tsx` (TSX, whose renderers need the Claude Code runtime) and runs them with
  node's type stripping.
- The SQL dumps (`sqlite3` `iterdump`) have the work-dir path replaced by `<work>`; the
  timestamps are the capture's.

Re-capture: `sh capture/run.sh <empty work dir> <inbox export> <panorama export> fixtures/inbox`.

## Known differences in the Rust port

Documented in the module docs; none occurs in anything the inbox writes, and only the negative
id (`question_ref`, `Q-1`) occurs in these goldens:

- Ids and versions are ASCII digits. Python's `\d` also matches other scripts' digits (the
  mod's TypeScript does not).
- An id over `u64::MAX` is dropped (Python's ints have no limit). A negative one (`Q-1`,
  which Python's `int()` reads as -1 and no question has) is refused by `question_id`.
- `\b` next to a `Q` id uses Rust's alphanumeric set, which also counts the combining marks
  with the Alphabetic property.
- `parse_open_args` takes the flags in full: argparse would also accept an abbreviation
  (`--quest`). Its error is the first problem met; argparse reports every unrecognized
  argument at once, after other errors, and quotes values with `repr()`. Only the message
  differs: the same argument lists are refused.
