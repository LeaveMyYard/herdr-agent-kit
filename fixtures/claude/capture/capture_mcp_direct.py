"""Speak MCP stdio directly to a plugin's MCP server (legacy `initialize` handshake, as an
older client would) and record request/reply lines. Usage: capture_mcp_direct.py <out.jsonl> -- <cmd...>"""
import json, subprocess, sys
out, cmd = sys.argv[1], sys.argv[sys.argv.index("--") + 1:]
reqs = [
    {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                                                                   "clientInfo": {"name": "capture", "version": "0"}}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
    {"jsonrpc": "2.0", "id": 1, "method": "tools/list"},
    {"jsonrpc": "2.0", "id": 2, "method": "no/such/method"},
    {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "no_such_tool", "arguments": {}}},
]
p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
with open(out, "w") as fh:
    for r in reqs:
        fh.write(json.dumps({"dir": "client->server", "msg": r}, ensure_ascii=False) + "\n")
        p.stdin.write(json.dumps(r) + "\n"); p.stdin.flush()
        if "id" in r:
            line = p.stdout.readline()
            fh.write(json.dumps({"dir": "server->client", "msg": json.loads(line)}, ensure_ascii=False) + "\n")
p.stdin.close(); p.wait(timeout=30)
