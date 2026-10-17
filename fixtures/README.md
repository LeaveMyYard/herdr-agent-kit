# fixtures

Test inputs shared by the kit's tests and, by tag, by both plugins.

**Everything here is captured, never invented**: from real herdr and Claude Code
runs, or from the running Python plugins. Each fixture says where and how it was
captured (a sibling `.md` or a header comment), so it can be re-captured.

Planned contents:

- `transcripts/`: Claude Code session transcripts, including continued and
  resumed sessions (they pin the continuation rule).
- `herdr/`: herdr socket request and reply recordings from an **isolated** herdr
  (its own `--session` and HOME). They seed `FakeHerdr`.
- `inbox/`: goldens of the inbox's published formats (`[inbox] …` delivery
  text, `ask_operator` receipts, `inbox open` arguments). herdr-inbox vendors a
  copy for its TS mod's tests and checks it against the pinned kit tag.

Never commit secrets or private content: scrub transcripts before adding them.
