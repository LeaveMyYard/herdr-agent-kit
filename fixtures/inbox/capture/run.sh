#!/bin/sh
# Re-capture the inbox-format goldens in fixtures/inbox (see capture.py for what and how).
#
#   sh run.sh <work-dir> <herdr-inbox-src> <herdr-panorama-src> <fixtures/inbox>
#
# <work-dir> is a fresh scratch directory. The two sources are clean exports of the plugin
# repos (`git archive origin/main | tar -x -C ...`), never a live plugin checkout. Nothing
# here talks to a real herdr or Claude Code: the env is `env -i` with no HERDR_* apart from
# the inbox's database path and its no-worker guard (both under <work-dir>), and a `herdr`
# stub that always fails comes first in PATH. Needs uv, node >= 23.6 and python3.
set -eu
WORK=$(cd "$1" && pwd); INBOX=$(cd "$2" && pwd); PANO=$(cd "$3" && pwd); OUT=$(cd "$4" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$WORK/bin" "$WORK/state" "$WORK/xdg-config"
printf '#!/bin/sh\necho "herdr disabled in the capture run" >&2\nexit 97\n' > "$WORK/bin/herdr"
chmod +x "$WORK/bin/herdr"
for p in inbox panorama; do
  src=$INBOX; [ $p = panorama ] && src=$PANO
  (cd "$src" && UV_PROJECT_ENVIRONMENT="$WORK/venv-$p" uv sync --quiet --frozen --no-dev)
done
rev() { git -C "$1" rev-parse HEAD 2>/dev/null || cat "$1/.capture-rev" 2>/dev/null || echo unknown; }
printf '{"herdr-inbox": "%s", "herdr-panorama": "%s", "node": "%s", "captured": "%s"}\n' \
  "$(rev "$INBOX")" "$(rev "$PANO")" "$(node --version)" "$(date -u +%Y-%m-%d)" > "$WORK/meta.json"
NODE_DIR=$(dirname "$(command -v node)")
exec env -i PATH="$WORK/bin:$NODE_DIR:/usr/bin:/bin" HOME="$WORK" LANG=C.UTF-8 \
  XDG_STATE_HOME="$WORK/state" XDG_CONFIG_HOME="$WORK/xdg-config" \
  HERDR_INBOX_DB="$WORK/state/inbox.db" HERDR_INBOX_NO_WORKER=1 \
  /usr/bin/python3 -I "$HERE/capture.py" "$WORK" "$INBOX" "$PANO" "$OUT"
