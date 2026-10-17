# fixtures

Test inputs shared by the kit's tests and, by tag, by both plugins.

**Everything here is captured, never invented**: from real herdr and Claude Code
runs, or from the running Python plugins. Each fixture says where and how it was
captured (a sibling `.md` or a header comment), so it can be re-captured.

Planned contents:

- `transcripts/`: captured Claude Code transcript record templates, one per record
  kind; the tests build transcripts (continued and resumed sessions included) only
  from them. See `transcripts/README.md`.
- `herdr/`: herdr socket request and reply recordings from an **isolated** herdr
  (its own HOME, XDG dirs and socket). They seed `FakeHerdr`. Captured: see
  `herdr/README.md` (method index included).
- `claude/`: Claude Code hook payloads for every hook event the two plugins register,
  and the MCP exchange (`tools/list` included) of both plugins' MCP servers. Captured:
  see `claude/README.md`.
- `toml10/`: what Python's `tomllib` (TOML 1.0) makes of TOML inputs, for the
  `config.toml` parser. Captured: see `toml10/README.md`.
- `inbox/`: goldens of the inbox's published formats (`[inbox] …` delivery
  text, `ask_operator` receipts, `inbox open` arguments) and of panorama's
  read-only `questions` reader. herdr-inbox vendors a copy for its TS mod's tests
  and checks it against the pinned kit tag. Captured: see `inbox/README.md`.

Never commit secrets or private content: scrub transcripts before adding them.

## Provenance

| dir | captured | from | how |
|---|---|---|---|
| `transcripts/` | Claude Code 2.1.x, copied 2026-10-17 | herdr-panorama `tests/fixtures/templates.json` at `4f525f3` | herdr-panorama's `capture.py`, anonymised; see `transcripts/README.md` |
| `herdr/` | 2026-10-17 | herdr 0.9.3, socket protocol 22 | isolated `herdr server` behind a recording proxy socket; `herdr/capture/capture_herdr.py` |
| `herdr/clean/` | 2026-10-17 | herdr-panorama `92f6d4c`, herdr-inbox `e209939` (`herdr.clean`) | both plugins' `clean()` run over one corpus, outputs required equal; `herdr/capture/capture_clean.py` |
| `claude/hooks/` | 2026-10-17 | Claude Code 2.1.302 (`claude-haiku-5-5`) | four short `claude -p` runs with a capture plugin mirroring the plugins' hook events and matchers; `claude/capture/run.sh` (assembled afterwards, not re-run end to end) |
| `claude/mcp/` | 2026-10-17 | herdr-inbox `a9b215a`, herdr-panorama `2cee57f` (Python `mcp` 2.3.0) | stdio tee between Claude Code and each server, plus a direct legacy `initialize` session |
| `toml10/` | 2026-10-17 | CPython 3.13.0 `tomllib`, toml-test `ff49d10` | `toml10/capture/capture_tomllib.py` |
| `inbox/` | 2026-10-17 | herdr-inbox `e209939`, herdr-panorama `92f6d4c` (bridged `main`), the herdr-inbox-ui mod at the same commit | the plugins' own functions, MCP server and TS parsers run over chosen inputs in an isolated env; `inbox/capture/run.sh` |

Each directory's README has the exact command lines, the isolation used and what, if
anything, was scrubbed. The live Claude Code runs were made with the operator's go (Q178).
