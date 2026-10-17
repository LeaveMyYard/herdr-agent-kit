# herdr-panorama model goldens

herdr-panorama's behaviour goldens for its model (plan `docs/plans/rust-rewrite.md` §5.2
item 3), copied unchanged from herdr-panorama main at `83590dc` (2026-10-17; the files last
changed in `d5408a7`, PR #57). They were captured from the bridged Python (0.1.1, PR #56) by
herdr-panorama's `tools/capture_goldens.py`; none is written by hand.

- `inputs/scenario-*.json`: each `tests/scenario.py` scenario as data: every file (its path
  under the scenario root, its mtime, its text), herdr's `api snapshot`, and the inbox rows.
- `model/*.json`: `panorama snapshot --json`, run as the real command in its own process with
  `time.time` held, over the scenario materialised under `<root>`, at the scenario's `now` plus
  each key's minutes (and, for the demo, with herdr down and with no inbox). Each file's
  `source` says exactly how.

`tests/panorama_model_goldens.rs` materialises each input the same way, builds the model with
the kit's `agent_session::claude` fold and a test-only port of panorama's model builder
(`tests/panorama_model/`), and writes the whole golden file again as the capture wrote it
(`json.dumps(..., indent=1, ensure_ascii=False)`): the bytes must match.

To re-capture, run `uv run python tools/capture_goldens.py` in herdr-panorama and copy
`tests/fixtures/inputs/scenario-*.json` and `tests/fixtures/model/*.json` here.
