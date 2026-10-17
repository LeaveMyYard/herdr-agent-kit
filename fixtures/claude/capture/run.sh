#!/bin/sh
# Re-capture the Claude Code hook payloads and MCP wire recordings in fixtures/claude.
#
#   sh run.sh <work-dir> <herdr-inbox-src> <herdr-panorama-src>
#
# <work-dir> is a fresh scratch directory. The two sources are clean exports of the plugin
# repos (`git archive origin/main | tar -x -C ...`), never a live plugin checkout.
# Four short `claude -p` runs on the cheapest model; they use the installed claude's own
# login and bill it. Nothing here reads, copies or passes a credential: the runs get a
# minimal `env -i` environment (no API key, no HERDR_* apart from the inbox's DB path and
# its no-worker guard, both under <work-dir>), and `--setting-sources project` keeps the
# user's settings (installed plugins, user hooks) out. A `herdr` stub that always fails
# comes first in PATH so nothing can reach a real herdr.
set -eu
WORK=$(cd "$1" && pwd); INBOX=$(cd "$2" && pwd); PANO=$(cd "$3" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$WORK/bin" "$WORK/cwd" "$WORK/state" "$WORK/xdg-config" "$WORK/out/hooks" "$WORK/out/mcp"
printf '#!/bin/sh\necho "herdr disabled in the capture run" >&2\nexit 97\n' > "$WORK/bin/herdr"
chmod +x "$WORK/bin/herdr"
for p in inbox panorama; do
  src=$INBOX; [ $p = panorama ] && src=$PANO
  (cd "$src" && UV_PROJECT_ENVIRONMENT="$WORK/venv-$p" uv sync --quiet --frozen --no-dev)
done
cat > "$WORK/mcp.json" <<EOF
{"mcpServers": {
 "herdr-inbox": {"command": "/usr/bin/python3", "args": ["-I", "$HERE/mcp_tee.py", "$WORK/out/mcp/herdr-inbox.wire.jsonl", "--",
   "uv", "run", "--quiet", "--frozen", "--no-dev", "--project", "$INBOX", "inbox", "mcp"],
   "env": {"UV_PROJECT_ENVIRONMENT": "$WORK/venv-inbox", "HERDR_INBOX_NO_WORKER": "1"}},
 "herdr-panorama": {"command": "/usr/bin/python3", "args": ["-I", "$HERE/mcp_tee.py", "$WORK/out/mcp/herdr-panorama.wire.jsonl", "--",
   "uv", "run", "--quiet", "--frozen", "--no-dev", "--project", "$PANO", "panorama", "mcp"],
   "env": {"UV_PROJECT_ENVIRONMENT": "$WORK/venv-panorama", "HERDR_INBOX_NO_WORKER": "1"}}}}
EOF
CLAUDE_BIN=$(command -v claude); UV_BIN_DIR=$(dirname "$(command -v uv)")
cenv() {
  env -i PATH="$WORK/bin:$(dirname "$CLAUDE_BIN"):$UV_BIN_DIR:/usr/bin:/bin" HOME="$HOME" USER="$USER" TERM=xterm-256color LANG=C.UTF-8 \
    XDG_STATE_HOME="$WORK/state" XDG_CONFIG_HOME="$WORK/xdg-config" HERDR_INBOX_DB="$WORK/state/inbox.db" HERDR_INBOX_NO_WORKER=1 \
    CAPTURE_OUT="$WORK/out/hooks" "$@"
}
S=$(cat /proc/sys/kernel/random/uuid 2>/dev/null || uuidgen | tr 'A-F' 'a-f')
COMMON="--model haiku --effort low --setting-sources project --strict-mcp-config --mcp-config $WORK/mcp.json
  --plugin-dir $HERE/plugin --output-format stream-json --verbose --include-hook-events"
cd "$WORK/cwd"
# 1: SessionStart(startup), SubagentStart (both matcher groups), PostToolBatch, SubagentStop, Stop x2
# shellcheck disable=SC2086
cenv claude -p "This is a synthetic capture run. Use the Agent tool exactly once, with subagent_type general-purpose, description 'capture subagent' and prompt 'Reply with the single word: ok'. After it returns, reply with the single word: done." \
  --session-id "$S" --allowedTools Agent $COMMON > "$WORK/out/run1.stream.jsonl"
# 2: SessionStart(resume) then SessionStart(compact)
# shellcheck disable=SC2086
cenv claude -p "/compact" --resume "$S" $COMMON > "$WORK/out/run2.stream.jsonl"
# 3: SessionStart(fork), Stop
# shellcheck disable=SC2086
cenv claude -p "Reply with the single word: ok" --resume "$S" --fork-session $COMMON > "$WORK/out/run3.stream.jsonl"
# 4: SessionStart(resume) then SessionStart(clear)
# shellcheck disable=SC2086
cenv claude -p "/clear" --resume "$S" $COMMON > "$WORK/out/run4.stream.jsonl"
# The legacy initialize handshake, spoken directly (no claude).
for p in inbox panorama; do
  src=$INBOX; [ $p = panorama ] && src=$PANO
  cenv env UV_PROJECT_ENVIRONMENT="$WORK/venv-$p" /usr/bin/python3 -I "$HERE/capture_mcp_direct.py" \
    "$WORK/out/mcp/herdr-$p.initialize-2025-06-18.jsonl" -- uv run --quiet --frozen --no-dev --project "$src" $p mcp
done
echo "hook payloads: $WORK/out/hooks; MCP wires: $WORK/out/mcp"
echo "the sessions' transcripts are under ~/.claude/projects/ (the cwd's slug): move them out when done"
