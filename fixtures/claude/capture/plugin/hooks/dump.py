"""Dump one hook payload (stdin JSON) plus the hook's CLAUDE_* environment to the capture dir.
Variables whose names mention a token, key, secret or auth are never read."""
import json, os, sys, time
label = sys.argv[1] if len(sys.argv) > 1 else "unknown"
raw = sys.stdin.read()
try:
    payload = json.loads(raw)
except ValueError:
    payload = None
deny = ("TOKEN", "KEY", "SECRET", "AUTH", "PASSWORD", "CREDENTIAL")
env = {k: os.environ[k] for k in sorted(os.environ)
       if k.startswith("CLAUDE") and not any(d in k for d in deny)}
names = sorted(k for k in os.environ if not any(d in k for d in deny))
out = os.environ["CAPTURE_OUT"]
n = time.time_ns()
with open(os.path.join(out, f"{n}-{label}.json"), "w") as fh:
    json.dump({"hook": label, "argv": sys.argv[1:], "stdin": payload if payload is not None else raw,
               "env": env, "env_names": names}, fh, indent=1)
