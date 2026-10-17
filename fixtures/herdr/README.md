# herdr socket and CLI recordings

Request and reply transcripts from an **isolated** herdr 0.9.3 (socket protocol 22), for
seeding `herdr_api::fake` (`FakeHerdr`) and for the CLI-path parser tests. Captured
2026-10-17 by `capture/capture_herdr.py`; nothing in these files was edited by hand.

## How they were captured

- `herdr server` ran with its own `HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME`,
  `XDG_CACHE_HOME`, `XDG_DATA_HOME`, `XDG_RUNTIME_DIR` and `HERDR_SOCKET_PATH`, all under the
  capture's work dir (the method of herdr-inbox `tools/latency_bench.py`), started from
  `env -i` so no other `HERDR_*` variable was set. The operator's herdr was never contacted.
- Every CLI call went to a **recording proxy** socket that forwards to the server socket
  and logs each newline-delimited JSON frame in both directions. So each CLI record holds
  the argv, exit code, stdout, stderr **and** the socket frames the CLI exchanged. Raw socket
  requests (the methods the plugins send without the CLI) went through the same proxy.
- The panes ran a bash with no rc files and the prompt `$ `. A stand-in `claude` script
  (in the capture's PATH) reports itself with `herdr pane report-agent` and echoes what is
  typed into it; it is not Claude Code, and no model ran.
- Two stub plugins were linked with `herdr plugin link`: they carry the real plugin ids
  (`leavemyyard.herdr-panorama`, `leavemyyard.herdr-inbox`), the real entrypoint ids
  (`panorama`, `new-agent`, `panel`, `inbox`) and an `open` action, with trivial commands.
  The popup entrypoints exit after 1 s, which is how a popup closes (popups are not in
  `pane list` and `plugin pane open` into a popup returns no pane id).
- No saved SSH machines: the `--machine` records are the "unknown machine" shapes.
- Re-capture: `env -i PATH=/usr/bin:/bin python3 -I capture/capture_herdr.py <out-dir> <work-dir>`
  (keep `<work-dir>` short: the socket path must fit in 108 bytes), then
  `python3 capture/index.py <out-dir>` prints the index below.

Paths in the records are the capture machine's work dir
(`/home/leavemyyard/Code/herdr-rust-conductor/scratch/P0a/recordings/iso/...`); pids,
terminal ids and timings are as recorded. No personal data beyond that path.

## Format

One JSON object per line; files group the calls by area (`tab.jsonl`, `pane.jsonl`, ...,
`socket.jsonl` for the raw socket methods, `events.jsonl`, `errors.jsonl`). `seq` is the
order within the file; the files together are one session, in the order of the index.

A CLI record:

```
{"seq", "kind": "cli", "name", "argv": ["herdr", ...], "exit_code", "stdout", "stdout_json",
 "stderr", "elapsed_ms", "socket": [{"conn", "dir": "request"|"reply", "frame"|"raw"}],
 "used_by"?, "note"?, "env"?}
```

`conn` numbers the socket connections within the record (the CLI opens one for its `ping`
and one for the call). A socket record:

```
{"seq", "kind": "socket", "name", "request", "replies": [...], "server_closed", "socket": [...],
 "used_by"?, "note"?}
```

`request` is the object sent as one line (a string when raw bytes were sent, as in the
malformed-JSON case). `replies` are the lines read back; for `events.subscribe` the first is
the ack and the rest are the events that the CLI calls recorded beside it in
`events.jsonl` caused.

`api-schema.json` is `herdr api schema --json` (the bundled JSON schema of every request,
reply and event; protocol 22, schema_version 1).

Things worth knowing that the records show:

- Errors come back on **stderr** as JSON (`{"error": {"code", "message"}, "id"}`) with exit 1;
  usage errors and `--machine <unknown>` exit 2 with plain text.
- `plugin pane open` into a popup replies `{"type": "ok"}` with no pane; a second popup
  while one is open fails with `ui_busy`.
- `pane swap` across tabs is not an error: it replies `changed: false` with a reason.
- `plugin config-dir` is answered by the CLI alone (no socket traffic), even for an unknown id.
- `pane report-metadata` errors carry the id `cli:request`.
- Socket errors for a request that does not parse carry `"id": ""`.

## Method index

`used_by` names the Python function (in `src/herdr_panorama/herdr.py` or
`src/herdr_inbox/herdr.py` unless said) that issues the same call.

| file:seq | name | call | socket methods on the wire | outcome | used by |
|---|---|---|---|---|---|
| meta:0 | version | `--version` |  | ok |  |
| meta:1 | api schema --json | `api schema --json` |  | ok |  |
| workspace:0 | workspace create | `workspace create --cwd …h/P0a/recordings/iso/proj --label capture --focus` | ping, workspace.create | ok `workspace_created` | tests/bench setup |
| workspace:1 | workspace list | `workspace list` | ping, workspace.list | ok `workspace_list` | inbox herdr.all_tabs |
| tab:0 | tab create (focus) | `tab create --workspace w1 --label Agent --cwd …h/P0a/recordings/iso/proj --focus` | ping, tab.create | ok `tab_created` | panorama herdr.create_tab |
| tab:1 | tab create (no-focus) | `tab create --workspace w1 --label Other --cwd …h/P0a/recordings/iso/proj --no-focus` | ping, tab.create | ok `tab_created` | panorama herdr.create_tab |
| tab:2 | tab list --workspace | `tab list --workspace w1` | ping, tab.list | ok `tab_list` | inbox herdr.all_tabs |
| tab:3 | tab get | `tab get w1:t2` | ping, tab.get | ok `tab_info` | inbox herdr.tab_label |
| tab:4 | tab rename | `tab rename w1:t3 ❓ Other` | ping, tab.rename | ok `tab_info` | both herdr.rename_tab |
| tab:5 | tab focus | `tab focus w1:t3` | ping, tab.focus | ok `tab_info` | panorama herdr.focus_tab |
| tab:6 | tab close | `tab close w1:t5` | ping, tab.close | ok `ok` | panorama herdr.close_tab |
| pane:0 | pane send-text | `pane send-text w1:p2 claude\n` | ping, pane.send_text | ok | tests/bench setup |
| pane:1 | pane report-agent-session | `pane report-agent-session w1:p2 --source capture --agent claude --agent-session-id 00000000-0000-4000-8000-000000000001 --session-start-source startup` | ping, pane.report_agent_session | ok | herdr's Claude Code integration (reference) |
| pane:2 | pane report-agent | `pane report-agent w1:p2 --source capture --agent claude --state idle --agent-session-id 00000000-0000-4000-8000-000000000001` | ping, pane.report_agent | ok | inbox tools/latency_bench.py, fake_agent.py |
| pane:3 | pane list | `pane list` | ping, pane.list | ok `pane_list` | inbox herdr.pane_tabs/focused_pane, panorama tab_root_pane |
| pane:4 | pane get (agent pane) | `pane get w1:p2` | ping, pane.get | ok `pane_info` | inbox herdr.pane_identity, panorama mcp_server |
| pane:5 | pane get (shell pane) | `pane get w1:p3` | ping, pane.get | ok `pane_info` | inbox herdr.pane_identity |
| pane:6 | pane process-info (agent pane) | `pane process-info --pane w1:p2` | ping, pane.process_info | ok `pane_process_info` | panorama herdr.claude_pid/runs_panorama |
| pane:7 | pane read visible 20 | `pane read w1:p2 --source visible --lines 20` | ping, pane.read | ok | inbox herdr.panel_focused |
| pane:8 | pane read visible 30 ansi | `pane read w1:p2 --source visible --lines 30 --format ansi` | ping, pane.read | ok | inbox herdr.composer_state |
| pane:9 | pane report-metadata --token | `pane report-metadata w1:p2 --source herdr-inbox --token inbox_q=?2` | ping, pane.report_metadata | ok | inbox herdr.set_open_count |
| pane:10 | pane report-metadata --clear-token | `pane report-metadata w1:p2 --source herdr-inbox --clear-token inbox_q` | ping, pane.report_metadata | ok | inbox herdr.set_open_count |
| pane:11 | pane read after prompt | `pane read w1:p2 --source visible --lines 20` | ping, pane.read | ok |  |
| pane:12 | pane rename | `pane rename w1:p3 capture pane` | ping, pane.rename | ok `pane_info` | panorama herdr.rename_pane |
| pane:13 | pane zoom --toggle (on) | `pane zoom w1:p3 --toggle` | ping, pane.zoom | ok `pane_zoom` | panorama herdr.zoom_pane |
| pane:14 | pane zoom --toggle (off) | `pane zoom w1:p3 --toggle` | ping, pane.zoom | ok `pane_zoom` | panorama herdr.zoom_pane |
| pane:15 | pane list (before popup) | `pane list` | ping, pane.list | ok `pane_list` |  |
| pane:16 | pane list (after the panorama popup exited) | `pane list` | ping, pane.list | ok `pane_list` |  |
| pane:17 | pane list (after the inbox popup exited) | `pane list` | ping, pane.list | ok `pane_list` |  |
| pane:18 | pane process-info (panel pane) | `pane process-info --pane w1:p6` | ping, pane.process_info | ok `pane_process_info` | panorama herdr.runs_panorama |
| pane:19 | pane list (before new-agent popup) | `pane list` | ping, pane.list | ok `pane_list` |  |
| pane:20 | pane list (after the new-agent popup exited) | `pane list` | ping, pane.list | ok `pane_list` |  |
| pane:21 | pane move --tab --split right | `pane move w1:p5 --tab w1:t2 --split right --no-focus --target-pane w1:p2` | ping, pane.move | ok `pane_move` | panorama herdr.move_pane_to_tab |
| pane:22 | pane swap | `pane swap --source-pane w1:p5 --target-pane w1:p2` | ping, pane.swap | ok `pane_swap` | panorama herdr.swap_panes |
| pane:23 | pane move --new-tab | `pane move w1:p5 --new-tab --label ◆ panorama --no-focus --workspace w1` | ping, pane.move | ok `pane_move` | panorama herdr.move_pane_new_tab |
| agent:0 | agent list | `agent list` | ping, agent.list | ok `agent_list` | inbox service/alerts |
| agent:1 | agent get (pane id) | `agent get w1:p2` | ping, agent.get | ok `agent_info` | inbox herdr.live_agent, panorama agent_exists |
| agent:2 | agent get (shell pane) | `agent get w1:p3` | ping, agent.get | exit 1 `agent_not_found` | inbox herdr.live_agent |
| agent:3 | agent focus | `agent focus w1:p2` | ping, agent.focus | ok `agent_info` | both herdr.focus_agent |
| agent:4 | agent prompt | `agent prompt w1:p2 capture prompt text` | ping, agent.prompt | ok `agent_prompted` | both herdr.prompt_agent |
| agent:5 | agent start (fake claude) | `agent start capture-agent --kind claude --pane w1:p8 --timeout 8000` | ping, pane.get, agent.start, agent.get | ok `agent_started` | panorama herdr.start_agent |
| agent:6 | agent get (by name) | `agent get capture-agent` | ping, agent.get | ok `agent_info` | panorama herdr.agent_exists |
| api:0 | api snapshot | `api snapshot` | ping, session.snapshot | ok `session_snapshot` | panorama herdr.snapshot |
| api:1 | api snapshot (after token) | `api snapshot` | ping, session.snapshot | ok `session_snapshot` | shows the token in the snapshot |
| plugin:0 | plugin link (panorama stub) | `plugin link …iso/plugins/panorama-stub` | ping, plugin.link | ok `plugin_linked` |  |
| plugin:1 | plugin link (inbox stub) | `plugin link …gs/iso/plugins/inbox-stub` | ping, plugin.link | ok `plugin_linked` |  |
| plugin:2 | plugin list --json | `plugin list --json` | ping, plugin.list | ok `plugin_list` | inbox herdr.plugin_root/radar, panorama inbox_link |
| plugin:3 | plugin config-dir | `plugin config-dir leavemyyard.herdr-panorama` |  | ok | both herdr.plugin_config_dir |
| plugin:4 | plugin action invoke (inbox open) | `plugin action invoke leavemyyard.herdr-inbox.open` | ping, plugin.action.invoke | ok `plugin_action_invoked` | panorama herdr.invoke_action |
| plugin:5 | plugin pane open (popup, default placement) | `plugin pane open --plugin leavemyyard.herdr-panorama --entrypoint panorama --cwd …iso/plugins/panorama-stub` | ping, plugin.pane.open | ok `ok` | panorama herdr.open_view |
| plugin:6 | plugin pane open (inbox popup, env) | `plugin pane open --plugin leavemyyard.herdr-inbox --entrypoint inbox --cwd …gs/iso/plugins/inbox-stub --env INBOX_FOCUS_PANE=w1:p2` | ping, plugin.pane.open | ok `ok` | inbox herdr.open_popup |
| plugin:7 | plugin pane open (split right of pane, focus) | `plugin pane open --plugin leavemyyard.herdr-panorama --entrypoint panorama --cwd …iso/plugins/panorama-stub --placement split --direction right --focus --target-pane w1:p2` | ping, plugin.pane.open | ok `plugin_pane_opened` | panorama herdr.open_view(split) |
| plugin:8 | plugin pane close (split) | `plugin pane close w1:p4` | ping, plugin.pane.close | ok `plugin_pane_closed` |  |
| plugin:9 | plugin pane open (tab, env, no-focus) | `plugin pane open --plugin leavemyyard.herdr-panorama --entrypoint panorama --placement tab --env PANORAMA_HOME=1 --no-focus --cwd …iso/plugins/panorama-stub` | ping, plugin.pane.open | ok `plugin_pane_opened` | panorama herdr.open_tab_pane |
| plugin:10 | plugin pane open (panel split, no-focus) | `plugin pane open --plugin leavemyyard.herdr-panorama --entrypoint panel --placement split --direction right --target-pane w1:p2 --no-focus --cwd …iso/plugins/panorama-stub` | ping, plugin.pane.open | ok `plugin_pane_opened` | panorama herdr.open_panel |
| notification:0 | notification show | `notification show Capture title --body capture body --sound request` | ping, notification.show | ok `notification_show` | inbox herdr.notify |
| machine:0 | machine list --json | `machine list --json` |  | ok | panorama machines, inbox remote |
| machine:1 | --machine unknown api snapshot | `--machine nosuch api snapshot` |  | exit 2 | panorama machines.snapshot |
| machine:2 | --machine unknown agent focus | `--machine nosuch agent focus w1:p2` |  | exit 2 | panorama tui remote focus |
| server:0 | server reload-config | `server reload-config` | ping, server.reload_config | ok `config_reload` | inbox radar |
| server:1 | server stop | `server stop` | server.stop | ok |  |
| socket:0 | pane.layout | socket `pane.layout` | pane.layout | ok `pane_layout` | panorama herdr.pane_layout |
| socket:1 | layout.export | socket `layout.export` | layout.export | ok `layout_export` | panorama herdr.layout_export |
| socket:2 | layout.set_split_ratio | socket `layout.set_split_ratio` | layout.set_split_ratio | ok `layout_split_ratio_set` | panorama herdr.set_split_ratio |
| socket:3 | pane.focus | socket `pane.focus` | pane.focus | ok `pane_info` | panorama herdr.focus_pane |
| socket:4 | tab.move | socket `tab.move` | tab.move | ok `tab_list` | panorama herdr.move_tab |
| socket:5 | plugin.pane.open (new-agent popup) | socket `plugin.pane.open` | plugin.pane.open | ok `ok` | panorama herdr.open_new_agent |
| socket:6 | layout.apply | socket `layout.apply` | layout.apply | ok `layout_apply` | panorama herdr.apply_panel_layout |
| events:0 | tab focus (during subscription) | `tab focus w1:t3` | ping, tab.focus | ok `tab_info` |  |
| events:1 | agent focus (during subscription) | `agent focus w1:p2` | ping, agent.focus | ok `agent_info` |  |
| events:2 | events.subscribe | socket `events.subscribe` | events.subscribe | ok `subscription_started` + 4 events | panorama herdr.events |
| errors:0 | plugin pane open (second popup while one is open) | `plugin pane open --plugin leavemyyard.herdr-inbox --entrypoint inbox --cwd …gs/iso/plugins/inbox-stub` | ping, plugin.pane.open | exit 1 `ui_busy` |  |
| errors:1 | pane get (unknown) | `pane get w9-p99` | ping, pane.get | exit 1 `pane_not_found` |  |
| errors:2 | pane get (malformed id) | `pane get not-a-pane` | ping, pane.get | exit 1 `pane_not_found` |  |
| errors:3 | tab get (unknown) | `tab get w9-t99` | ping, tab.get | exit 1 `tab_not_found` |  |
| errors:4 | tab rename (unknown) | `tab rename w9-t99 x` | ping, tab.rename | exit 1 `tab_not_found` |  |
| errors:5 | tab focus (unknown) | `tab focus w9-t99` | ping, tab.focus | exit 1 `tab_not_found` |  |
| errors:6 | tab close (unknown) | `tab close w9-t99` | ping, tab.close | exit 1 `tab_not_found` |  |
| errors:7 | tab list (unknown workspace) | `tab list --workspace w99` | ping, tab.list | exit 1 `workspace_not_found` |  |
| errors:8 | agent get (no agent) | `agent get nobody` | ping, agent.get | exit 1 `agent_not_found` |  |
| errors:9 | agent focus (shell pane) | `agent focus w1:p3` | ping, agent.focus | exit 1 `agent_not_found` |  |
| errors:10 | agent prompt (shell pane) | `agent prompt w1:p3 hello` | ping, agent.prompt | exit 1 `agent_not_found` |  |
| errors:11 | agent start (unknown pane) | `agent start x --kind claude --pane w9-p99 --timeout 1000` | ping, pane.get, agent.start | exit 1 `agent_pane_not_found` |  |
| errors:12 | pane process-info (unknown) | `pane process-info --pane w9-p99` | ping, pane.process_info | exit 1 `pane_not_found` |  |
| errors:13 | pane read (unknown) | `pane read w9-p99 --source visible --lines 20` | ping, pane.read | exit 1 `pane_not_found` |  |
| errors:14 | pane report-metadata (unknown) | `pane report-metadata w9-p99 --source herdr-inbox --token inbox_q=?1` | ping, pane.report_metadata | exit 1 `pane_not_found` |  |
| errors:15 | pane move (unknown tab) | `pane move w1:p3 --tab w9-t99 --split right --no-focus` | ping, pane.move | exit 1 `tab_not_found` |  |
| errors:16 | pane swap (across tabs: ok, changed=false) | `pane swap --source-pane w1:p2 --target-pane w1:p3` | ping, pane.swap | ok `pane_swap` |  |
| errors:17 | pane zoom (unknown) | `pane zoom w9-p99 --toggle` | ping, pane.zoom | exit 1 `pane_not_found` |  |
| errors:18 | pane rename (unknown) | `pane rename w9-p99 x` | ping, pane.rename | exit 1 `pane_not_found` |  |
| errors:19 | plugin pane open (unknown plugin) | `plugin pane open --plugin nobody.none --entrypoint x` | ping, plugin.pane.open | exit 1 `plugin_not_found` |  |
| errors:20 | plugin pane open (unknown entrypoint) | `plugin pane open --plugin leavemyyard.herdr-panorama --entrypoint nope` | ping, plugin.pane.open | exit 1 `plugin_pane_not_found` |  |
| errors:21 | plugin pane close (not a plugin pane) | `plugin pane close w1:p3` | ping, plugin.pane.close | exit 1 `plugin_pane_not_found` |  |
| errors:22 | plugin action invoke (unknown) | `plugin action invoke nobody.none.open` | ping, plugin.action.invoke | exit 1 `plugin_action_not_found` |  |
| errors:23 | plugin config-dir (unknown) | `plugin config-dir nobody.none` |  | ok |  |
| errors:24 | unknown subcommand | `pane frobnicate` |  | exit 2 |  |
| errors:25 | socket: unknown method | socket `no.such.method` | no.such.method | error `invalid_request` |  |
| errors:26 | socket: invalid params | socket `tab.move` | tab.move | error `invalid_request` |  |
| errors:27 | socket: unknown tab | socket `tab.move` | tab.move | error `tab_not_found` |  |
| errors:28 | socket: unknown pane (pane.layout) | socket `pane.layout` | pane.layout | error `pane_not_found` |  |
| errors:29 | socket: unknown pane (pane.focus) | socket `pane.focus` | pane.focus | error `pane_not_found` |  |
| errors:30 | socket: unknown tab (layout.export) | socket `layout.export` | layout.export | error `layout_not_found` |  |
| errors:31 | socket: bad split path | socket `layout.set_split_ratio` | layout.set_split_ratio | error `split_not_found` |  |
| errors:32 | socket: plugin.pane.open unknown plugin | socket `plugin.pane.open` | plugin.pane.open | error `plugin_not_found` |  |
| errors:33 | socket: events.subscribe unknown type | socket `events.subscribe` | events.subscribe | error `invalid_request` |  |
| errors:34 | socket: malformed JSON | socket `(raw)` |  | error `invalid_request` |  |
| errors:35 | socket: missing id | socket `pane.focus` | pane.focus | error `invalid_request` |  |
| errors:36 | socket: missing params | socket `pane.focus` | pane.focus | error `invalid_request` |  |
