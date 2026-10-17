# Claude Code transcript record templates

`templates.json` is herdr-panorama's `tests/fixtures/templates.json`, copied unchanged from
herdr-panorama main at `f504b95` (2026-10-17). It holds one real record per kind the plugins
read (main and subagent transcript records, task-notification carriers, agent meta files,
workflow journal and run summary, and the `continued-in`, `last-prompt`, `cost-state`,
`mode` and `file-history-snapshot` records), captured from real Claude Code 2.1.x transcripts
by herdr-panorama's `tests/fixtures/capture.py` and anonymised there: keys, nesting, value
types and the structural values the parsers key on are kept; every id, timestamp, path and
piece of text is replaced. See herdr-panorama `tests/fixtures/README.md` for the capture
commands.

The kit's tests (`tests/common/mod.rs`) build transcripts only from these templates, filling
in ids, times and text, as herdr-panorama's `tests/scenario.py` does. To re-capture, run
herdr-panorama's `capture.py` and copy the result here.
