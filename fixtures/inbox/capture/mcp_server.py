"""herdr-inbox's own MCP server with one test double: the caller (run in the inbox's venv).

    python mcp_server.py

``service.resolve_caller`` asks herdr and walks the process tree for the asking agent's pane;
the capture has no herdr, so it is replaced by a fixed Caller built from CAPTURE_PANE /
CAPTURE_SESSION (no CAPTURE_PANE: a caller outside herdr, as resolve_caller reports one).
Everything else, the tool functions and the MCP SDK's serialization of their results, is the
server as shipped. Run only from capture.py (isolated env, failing herdr stub).
"""

import os

from herdr_inbox import server, service

assert not any(k.startswith("HERDR_") and k not in ("HERDR_INBOX_DB", "HERDR_INBOX_NO_WORKER") for k in os.environ)
caller = service.Caller(pane_id=os.environ.get("CAPTURE_PANE") or None, tab_id="w1:t1", tab_label="capture",
                        session_id=os.environ.get("CAPTURE_SESSION") or None, cwd="/capture/cwd")
service.resolve_caller = lambda: caller
service.enable_background()
server.mcp.run("stdio")
