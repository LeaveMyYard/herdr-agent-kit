# Differential replay

Runs the Python and Rust session scanners side by side over your real Claude
Code transcripts at past instants, and prints where they disagree. This is the
parity check of plan §5.2 item 4 (herdr-panorama
`docs/plans/rust-rewrite.md`). The exit criterion is zero status and zero
parent disagreements.

- **Python side:** herdr-panorama's `scan.session.Session` from a checkout of
  panorama `main` (the bridged scanner).
- **Rust side:** `claude_session::Scanner` and `Session::status_of` /
  `parent_of` from this kit, through `examples/replay_scan.rs`.

## Method

It reuses panorama's replay oracle (`tools/replay_oracle.py`): its `Truth`
class lists a session's files, and `Truth.truncate(T, dest)` copies every file
cut at instant `T`. For each session with subagents (default: all of them;
`--newest N` for the newest N, or session-id prefixes as arguments):

1. The instants are the oracle's: K spread over the session and K inside
   activity gaps (a random agent write plus 2 to 30 minutes), seeded by the
   session path, so every run uses the same ones. Two more instants are added:
   the session's last record, and one hour after it.
2. At each instant the files are copied, cut at `T`, into `--scratch`. Each
   copy's mtime is set to what it was at `T` (a transcript's last kept record
   time, any other file's own mtime capped at `T`), because both scanners read
   mtimes (the `since` skip, the workflow summary evidence).
3. panorama's hook records of the session (`~/.local/state/herdr-panorama/hooks`,
   or `--hooks-dir`) are cut at `T` too and given to both sides.
4. Both scanners read the same copy with `now = T` and
   `since = T - 3600 - 600` (the oracle's window). Process liveness is unknown
   on both sides: the processes of a past instant cannot be checked (Python's
   `hooks.process_alive` returns `None`; Rust uses `NoProcs`).
5. For every agent either side knows, the harness compares `present` (known to
   both), `status`, `parent`, `inferred` and `ended` (to 1 ms).

The copies hold transcript content, so they live only under `--scratch` and
are deleted after each session. Nothing from them is printed or committed.

## Output

Counts only, plus one line per disagreement with the session id prefix, the
instant (seconds after the session's first record), the agent id, the field
and the two values. Never content, prompts or paths. One summary line per
session and a `TOTAL` line, which includes how many agent-instants had each
Python status. Exit status 1 when any `present`, `status` or `parent`
disagreement remains, else 0.

## Running it

Unset every `HERDR_*` variable first. The transcripts are read only.

```sh
# Python side: a checkout of herdr-panorama main with its own venv
git -C ~/Code/herdr-panorama worktree add --detach <panorama-wt> origin/main
(cd <panorama-wt> && uv sync)

# Rust side
cargo build --release --example replay_scan

<panorama-wt>/.venv/bin/python tools/replay/replay_diff.py \
    --panorama <panorama-wt> \
    --rust target/release/examples/replay_scan \
    --scratch <empty scratch dir>
```

Options: `--instants K` (default 24), `--newest N` (default 0, all),
`--projects DIR` (default `~/.claude/projects`), `--hooks-dir DIR`, and
session-id prefixes to add.

## Classifying a disagreement

Each disagreement is a Rust bug, a Python bug or a harness artefact. A Rust bug
is fixed in the kit. A Python bug is filed on herdr-panorama: both sides then
change together, so that the rule stays one rule.
