# Claude Code hook payloads and MCP recordings

Captured 2026-10-17 (about 14:14 UTC) from **Claude Code 2.1.302** (`claude --version`),
model `claude-haiku-5-5`, with the operator's go (Q178). Nothing in these files was edited
by hand; the `*.tools-list.json` files are cut out of the recorded `tools/list` replies.

## Hook payloads (`hooks/`)

One file per payload, as the capture hook wrote it:

```
{"hook": <capture label>, "argv": [...], "stdin": <the payload Claude Code sent>,
 "env": {<CLAUDE* variables the hook saw>}, "env_names": [<every variable name>]}
```

The capture plugin (`capture/plugin`) registers the same events, matchers and exec form
(`"command": "sh", "args": [...]`) as the two plugins' `hooks/hooks.json`, with
`capture/plugin/hooks/dump.py` in place of the plugin's script. Timeouts and the exit
mapping differ (the rewake hook has `timeout: 60` instead of 43200 and no
`[ $? -eq 3 ] && exit 2`; the inbox SessionStart timeout is 10 instead of 20). The payloads
do not depend on them, nor on the hook command itself: herdr-panorama's main has since
moved its hooks to a `run-hook.sh` shim (#51) with the same events and matchers, and that
needs no re-capture. Variables whose names mention
a token, key, secret, auth, password or credential were never read. `CAPTURE_OUT`,
`HERDR_INBOX_DB` and `HERDR_INBOX_NO_WORKER` in `env_names` are the capture's own.

| file | event | registered by | run |
|---|---|---|---|
| `SessionStart.startup.json` | SessionStart `startup` | both (`startup\|resume\|clear\|compact\|fork`) | 1 |
| `SubagentStart.panorama-matcher-group.json` | SubagentStart | panorama (matcher `.+`) | 1 |
| `SubagentStart.inbox-matcher-group.json` | SubagentStart | inbox (no matcher) | 1 |
| `PostToolBatch.json` | PostToolBatch (one `Agent` call) | inbox | 1 |
| `SubagentStop.json` | SubagentStop | panorama (matcher `.+`) | 1 |
| `Stop.run1-turn1.json` | Stop, while the background subagent ran | inbox | 1 |
| `Stop.run1-turn2.json` | Stop, after the subagent's result came back | inbox | 1 |
| `Stop.rewake-watch.run1-turn1.json`, `...turn2.json` | the same, seen by the `asyncRewake` hook | inbox | 1 |
| `SessionStart.resume.json` | SessionStart `resume` (`--resume`) | both | 2 |
| `SessionStart.compact.json` | SessionStart `compact` (`/compact`) | both | 2 |
| `SessionStart.fork.json` | SessionStart `fork` (`--resume --fork-session`) | both | 3 |
| `Stop.fork.json`, `Stop.rewake-watch.fork.json` | Stop in the forked session | inbox | 3 |
| `SessionStart.resume.before-clear.json` | SessionStart `resume` | both | 4 |
| `SessionStart.clear.json` | SessionStart `clear` (`/clear`, new session id) | both | 4 |

Notes the payloads show: Claude Code ran the `Agent` call as a background agent, so the
first `Stop` came before `SubagentStop`; `SessionStart` for `resume` and `fork` carries
`seconds_since_last_response`, `context_tokens`, `prompt_cache_likely_expired` and
`estimated_cache_write_usd`, and for `compact` a `prompt_id` and `model`. The session ids are
the capture's own (`4f58e439-…` main, `f75d153d-…` fork, `574be6ec-…` after `/clear`).

## MCP (`mcp/`)

- `herdr-<plugin>.claude-code-2.1.302.jsonl`: every JSON-RPC line between Claude Code and
  the plugin's MCP server in run 1, recorded by a stdio tee (`capture/mcp_tee.py`) wrapped
  around the server command: `{"t", "dir": "client->server"|"server->client", "msg"}`.
  Claude Code 2.1.302 speaks MCP protocol `2026-07-28` here: `server/discover`,
  `subscriptions/listen`, then `prompts/list`, `resources/list`, `tools/list`, with no
  `initialize`. Runs 2 to 4 sent the identical exchange (checked, timestamps aside).
- `herdr-<plugin>.initialize-2025-06-18.jsonl`: the legacy handshake spoken directly to the
  server without Claude Code (`capture/capture_mcp_direct.py`): `initialize`,
  `notifications/initialized`, `tools/list`, an unknown method and an unknown tool.
- `herdr-<plugin>.tools-list.json`: the `tools` array of the Claude Code `tools/list` reply.

The servers ran from clean `git archive` exports of the plugin repos' `main`
(herdr-inbox `a9b215a6d8a3153ac929c6eaa9fc9c831e7b174b`, herdr-panorama
`2cee57ffd66ee9c0b236480e3cb1d03815141238`) in their own venvs (Python `mcp` 2.3.0), with
`HERDR_INBOX_NO_WORKER=1` so no delivery worker started.

## How the runs were isolated

- Four short `claude -p` runs with the installed claude's existing login (Q178). No
  credential file or token was read, copied or passed: each run started from `env -i` with
  only `PATH`, `HOME`, `USER`, `TERM`, `LANG`, `XDG_STATE_HOME` and `XDG_CONFIG_HOME` (both
  in the capture's work dir), `HERDR_INBOX_DB` (work dir), `HERDR_INBOX_NO_WORKER=1`,
  `UV_CACHE_DIR` and `CAPTURE_OUT`. No other `HERDR_*` variable was set, and a `herdr` stub
  that always fails came first in `PATH`.
- `--setting-sources project` (the cwd was an empty work dir) kept the user's settings out,
  so neither the installed plugins nor the user's hooks loaded; `--strict-mcp-config` with
  `--mcp-config` loaded only the two tee-wrapped servers; `--plugin-dir` loaded only the
  capture plugin. The run-1 init message listed exactly `herdr-capture` plus Claude Code's
  built-ins, and its hook events listed only the capture hooks.
- The operator's inbox database was checked with `sqlite3 -readonly` before and after: the
  row counts of `questions`, `sessions`, `alerts` and `changes` were unchanged, and no row
  holds any of the three capture session ids.
- Only `--allowedTools Agent` was granted; no permission-bypass flag.
- The command lines are in `capture/run.sh`, which re-creates the setup. It was assembled
  from the commands used, after the capture, and has **not been re-run end to end** (each
  run bills the operator). Differences from the recorded runs: the capture passed the
  plugin as a copy (`.../recordings/claude/capture-plugin`, which is the
  `CLAUDE_PLUGIN_ROOT` in the payloads) where run.sh passes `capture/plugin`; `UV_CACHE_DIR`
  is now set in run.sh as it was in the capture.

```
claude -p "<synthetic prompt: use the Agent tool once, then reply done>" --session-id <S> --allowedTools Agent $COMMON
claude -p "/compact" --resume <S> $COMMON
claude -p "Reply with the single word: ok" --resume <S> --fork-session $COMMON
claude -p "/clear" --resume <S> $COMMON
COMMON = --model haiku --effort low --setting-sources project --strict-mcp-config --mcp-config <mcp.json>
         --plugin-dir capture/plugin --output-format stream-json --verbose --include-hook-events
```

Afterwards the three sessions' transcripts were moved out of `~/.claude/projects/` (and
their `session-env` and plugin-data dirs removed); they are not committed here.

Paths in the payloads are the capture machine's (`/home/leavemyyard/...`); the prompt text is
the synthetic prompt above, and the assistant messages are the model's replies to it.
