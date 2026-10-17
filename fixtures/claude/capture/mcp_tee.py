"""stdio MCP tee: run the real server command, relay stdin/stdout unchanged, and log every
JSON-RPC line in both directions to <log>. Usage: mcp_tee.py <log.jsonl> -- <command...>"""
import json, subprocess, sys, threading, time
log_path, cmd = sys.argv[1], sys.argv[sys.argv.index("--") + 1:]
lock = threading.Lock()
logf = open(log_path, "a")
def log(direction, line):
    try:
        msg = json.loads(line)
    except ValueError:
        msg = {"_raw": line.decode("utf-8", "replace") if isinstance(line, bytes) else line}
    with lock:
        logf.write(json.dumps({"t": round(time.time(), 3), "dir": direction, "msg": msg}, ensure_ascii=False) + "\n")
        logf.flush()
p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
def up():
    for line in sys.stdin.buffer:
        log("client->server", line)
        try:
            p.stdin.write(line); p.stdin.flush()
        except BrokenPipeError:
            break
    try:
        p.stdin.close()
    except OSError:
        pass
threading.Thread(target=up, daemon=True).start()
for line in p.stdout:
    log("server->client", line)
    sys.stdout.buffer.write(line); sys.stdout.buffer.flush()
sys.exit(p.wait())
