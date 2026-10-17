"""Record herdr socket request/reply transcripts and CLI outputs on an ISOLATED herdr.

    env -i PATH=/usr/bin:/bin python3 -I capture_herdr.py <out-dir> <work-dir>

The isolated server gets its own HOME, XDG_* dirs, XDG_RUNTIME_DIR and socket under
<work-dir>/iso (as herdr-inbox tools/latency_bench.py does). Keep <work-dir> short: the
socket path must fit in 108 bytes. Never run it with any HERDR_* variable set. Every CLI call is pointed at a
recording proxy socket that forwards to the server's socket and logs each newline-delimited
JSON frame in both directions, so a CLI call is recorded as argv + stdout/stderr/exit +
the exact socket frames the CLI exchanged. Raw socket requests (the methods the plugins
send directly) go through the same proxy.
"""
from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = Path(sys.argv[1]).resolve()
ROOT = Path(sys.argv[2]).resolve()
ISO = ROOT / "iso"
RUN = ISO / "run"
SERVER_SOCK = RUN / "herdr.sock"
PROXY_SOCK = RUN / "proxy.sock"

assert not any(k.startswith("HERDR_") for k in os.environ), "unset every HERDR_* first"


# -- setup ------------------------------------------------------------------------------

def setup() -> dict:
    shutil.rmtree(ISO, ignore_errors=True)
    home = ISO / "home"
    for d in ("home/.config/herdr", "home/.local/state", "home/.cache", "bin", "proj", "run",
              "plugins/panorama-stub", "plugins/inbox-stub"):
        (ISO / d).mkdir(parents=True, exist_ok=True)
    RUN.chmod(0o700)
    (home / ".config/herdr/config.toml").write_text(
        'onboarding = false\n[update]\nversion_check = false\nmanifest_check = false\n'
        f'[terminal]\ndefault_shell = "{ISO / "bin" / "capshell"}"\n')
    # a bash without rc files and with a neutral prompt (no user or host name in pane reads)
    shell = ISO / "bin" / "capshell"
    shell.write_text("#!/bin/sh\nexec env PS1='$ ' /bin/bash --norc --noprofile -i \"$@\"\n")
    shell.chmod(0o755)
    # A stand-in `claude`: reports itself to herdr as an idle claude agent with a fixed
    # session id, then echoes what is typed into it. No model, no network.
    fake = ISO / "bin" / "claude"
    fake.write_text(
        "#!/bin/sh\n"
        "herdr pane report-agent \"$HERDR_PANE_ID\" --source capture --agent claude --state idle "
        "--agent-session-id 00000000-0000-4000-8000-000000000001 >/dev/null 2>&1\n"
        "echo 'fake claude ready'\n"
        "while IFS= read -r line; do echo \"got: $line\"; done\n")
    fake.chmod(0o755)
    # A stand-in `panorama` so `pane process-info` shows argv [... panorama panel].
    pano = ISO / "bin" / "panorama"
    pano.write_text("#!/bin/sh\nwhile :; do sleep 60; done\n")
    pano.chmod(0o755)
    # Stub plugins with the real plugin ids and entrypoint ids, linked with `herdr plugin link`.
    (ISO / "plugins/panorama-stub/herdr-plugin.toml").write_text(
        'id = "leavemyyard.herdr-panorama"\nname = "panorama stub"\nversion = "0.0.0"\n'
        'min_herdr_version = "0.9.0"\ndescription = "capture stub"\nplatforms = ["linux", "macos"]\n'
        + "".join(
            f'\n[[panes]]\nid = "{pid}"\ntitle = "{pid}"\nplacement = "{pl}"\n'
            f'command = {cmd}\n'
            for pid, pl, cmd in (
                # while the flag file exists the view exits after 1 s: how a popup goes away
                ("panorama", "popup", f'["sh", "-c", "[ -e {ISO}/popup-exits ] && exec sleep 1; exec panorama tui"]'),
                ("new-agent", "popup", '["sh", "-c", "exec sleep 1"]'),
                ("panel", "split", '["panorama", "panel"]')))
        + '\n[[actions]]\nid = "open"\ntitle = "open"\ndescription = "stub"\ncommand = ["true"]\n')
    (ISO / "plugins/inbox-stub/herdr-plugin.toml").write_text(
        'id = "leavemyyard.herdr-inbox"\nname = "inbox stub"\nversion = "0.0.0"\n'
        'min_herdr_version = "0.9.0"\ndescription = "capture stub"\nplatforms = ["linux", "macos"]\n'
        '\n[[panes]]\nid = "inbox"\ntitle = "inbox"\nplacement = "popup"\ncommand = ["sh", "-c", "exec sleep 1"]\n'
        '\n[[actions]]\nid = "open"\ntitle = "open"\ndescription = "stub"\ncommand = ["true"]\n')
    return {
        "PATH": f"{ISO / 'bin'}:/usr/bin:/bin", "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"), "XDG_STATE_HOME": str(home / ".local/state"),
        "XDG_CACHE_HOME": str(home / ".cache"), "XDG_DATA_HOME": str(home / ".local/share"),
        "XDG_RUNTIME_DIR": str(RUN), "HERDR_SOCKET_PATH": str(SERVER_SOCK),
        "TERM": "xterm-256color", "LANG": "C.UTF-8", "USER": "capture", "SHELL": "/bin/bash",
    }


# -- recording proxy --------------------------------------------------------------------

FRAMES: list[dict] = []
LOCK = threading.Lock()
CONN = [0]


def _pump(src: socket.socket, dst: socket.socket, conn: int, direction: str) -> None:
    buf = b""
    try:
        while True:
            chunk = src.recv(65536)
            if not chunk:
                break
            dst.sendall(chunk)
            buf += chunk
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                _log(conn, direction, line)
    except OSError:
        pass
    if buf:
        _log(conn, direction, buf, partial=True)
    for s in (dst,):
        try:
            s.shutdown(socket.SHUT_WR)
        except OSError:
            pass


def _log(conn: int, direction: str, line: bytes, partial: bool = False) -> None:
    text = line.decode("utf-8", "replace")
    try:
        frame = json.loads(text)
    except ValueError:
        frame = None
    rec = {"conn": conn, "dir": direction, "t": time.monotonic()}
    if frame is not None:
        rec["frame"] = frame
    else:
        rec["raw"] = text
    if partial:
        rec["unterminated"] = True
    with LOCK:
        FRAMES.append(rec)


def proxy() -> None:
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(str(PROXY_SOCK))
    srv.listen(64)
    while True:
        client, _ = srv.accept()
        CONN[0] += 1
        conn = CONN[0]
        up = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            up.connect(str(SERVER_SOCK))
        except OSError:  # the server is gone (after `server stop`)
            client.close()
            continue
        threading.Thread(target=_pump, args=(client, up, conn, "request"), daemon=True).start()
        threading.Thread(target=_pump, args=(up, client, conn, "reply"), daemon=True).start()


def frames_since(mark: int, settle: float = 0.15) -> list[dict]:
    time.sleep(settle)
    with LOCK:
        older = {f["conn"] for f in FRAMES[:mark]}  # e.g. a live event subscription
        out = [f for f in FRAMES[mark:] if f["conn"] not in older]
    base = {}
    res = []
    for f in out:
        base.setdefault(f["conn"], len(base) + 1)
        g = {k: v for k, v in f.items() if k != "t"}
        g["conn"] = base[f["conn"]]  # renumbered per record
        res.append(g)
    return res


def mark() -> int:
    with LOCK:
        return len(FRAMES)


# -- recorders ------------------------------------------------------------------------

RECORDS: dict[str, list[dict]] = {}
ENV: dict = {}


def put(file: str, rec: dict) -> dict:
    RECORDS.setdefault(file, []).append(rec)
    return rec


def cli(file: str, name: str, *args: str, used_by: str = "", note: str = "", timeout: float = 20,
        env_extra: dict | None = None) -> dict:
    env = {**ENV, "HERDR_SOCKET_PATH": str(PROXY_SOCK), **(env_extra or {})}
    m = mark()
    t0 = time.perf_counter()
    try:
        p = subprocess.run(["herdr", *args], env=env, capture_output=True, text=True, timeout=timeout)
        code, out, err = p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired as exc:
        code, out, err = None, exc.stdout or "", f"[capture: timed out after {timeout}s]"
        out = out.decode() if isinstance(out, bytes) else out
    ms = round((time.perf_counter() - t0) * 1000, 1)
    rec = {"kind": "cli", "name": name, "argv": ["herdr", *args], "exit_code": code, "stdout": out,
           "stderr": err, "elapsed_ms": ms}
    try:
        rec["stdout_json"] = json.loads(out) if out.strip().startswith(("{", "[")) else None
    except ValueError:
        rec["stdout_json"] = None
    rec["socket"] = frames_since(m)
    if env_extra:
        rec["env"] = env_extra
    if used_by:
        rec["used_by"] = used_by
    if note:
        rec["note"] = note
    return put(file, rec)


def result(rec: dict) -> dict:
    return ((rec.get("stdout_json") or {}).get("result")) or {}


def sock(file: str, name: str, request, used_by: str = "", note: str = "", read_for: float = 3.0,
         until_reply: bool = True, during=None) -> dict:
    """One raw socket request (a dict is sent as one JSON line, a str is sent verbatim)."""
    m = mark()
    line = (json.dumps(request) + "\n") if not isinstance(request, (str, bytes)) else request
    data = line.encode() if isinstance(line, str) else line
    replies, closed = [], False
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(0.2)
        s.connect(str(PROXY_SOCK))
        s.sendall(data)
        buf, end = b"", time.monotonic() + read_for
        fired = False
        while time.monotonic() < end:
            if during and replies and not fired:
                fired = True
                during()
            try:
                chunk = s.recv(65536)
            except socket.timeout:
                continue
            if not chunk:
                closed = True
                break
            buf += chunk
            while b"\n" in buf:
                ln, buf = buf.split(b"\n", 1)
                try:
                    replies.append(json.loads(ln))
                except ValueError:
                    replies.append({"_raw": ln.decode("utf-8", "replace")})
            if until_reply and replies and not during:
                break
    rec = {"kind": "socket", "name": name, "request": request if not isinstance(request, bytes) else request.decode(),
           "replies": replies, "server_closed": closed}
    if used_by:
        rec["used_by"] = used_by
    if note:
        rec["note"] = note
    rec["socket"] = [f for f in frames_since(m) if f["conn"] == 1]  # this request's connection only
    return put(file, rec)


# -- the session ------------------------------------------------------------------------

def main() -> None:
    global ENV
    ENV = setup()
    OUT.mkdir(parents=True, exist_ok=True)
    for f in OUT.glob("*.jsonl"):
        f.unlink()
    server = subprocess.Popen(["herdr", "server"], env=ENV, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                              start_new_session=True)
    try:
        for _ in range(100):
            if SERVER_SOCK.exists():
                break
            time.sleep(0.1)
        time.sleep(0.5)
        threading.Thread(target=proxy, daemon=True).start()
        for _ in range(50):
            if PROXY_SOCK.exists():
                break
            time.sleep(0.05)
        session()
    finally:
        def unlink_proxy_when_server_gone():
            # the CLI waits until the socket it used is unreachable: mirror the server's
            while SERVER_SOCK.exists():
                time.sleep(0.05)
            PROXY_SOCK.unlink(missing_ok=True)
        threading.Thread(target=unlink_proxy_when_server_gone, daemon=True).start()
        cli("server", "server stop", "server", "stop", note="ends the capture")
        try:
            server.wait(timeout=10)
        except subprocess.TimeoutExpired:
            server.kill()
        for file, recs in RECORDS.items():
            with open(OUT / f"{file}.jsonl", "w") as fh:
                for i, r in enumerate(recs):
                    fh.write(json.dumps({"seq": i, **r}, ensure_ascii=False) + "\n")
        print(json.dumps({k: len(v) for k, v in RECORDS.items()}))


def session() -> None:
    P = "leavemyyard.herdr-panorama"
    I = "leavemyyard.herdr-inbox"
    proj = str(ISO / "proj")
    # metadata and setup
    cli("meta", "version", "--version")
    r = cli("meta", "api schema --json", "api", "schema", "--json")
    (OUT / "api-schema.json").write_text(r["stdout"])
    r["stdout"] = "[saved as api-schema.json]"
    r["stdout_json"] = None
    cli("plugin", "plugin link (panorama stub)", "plugin", "link", str(ISO / "plugins/panorama-stub"),
        note="setup: stub with the real plugin id and entrypoint ids")
    cli("plugin", "plugin link (inbox stub)", "plugin", "link", str(ISO / "plugins/inbox-stub"),
        note="setup: stub with the real plugin id and entrypoint ids")

    ws = result(cli("workspace", "workspace create", "workspace", "create", "--cwd", proj, "--label", "capture",
                    "--focus", used_by="tests/bench setup"))
    wid = ws["workspace"]["workspace_id"]
    t1 = result(cli("tab", "tab create (focus)", "tab", "create", "--workspace", wid, "--label", "Agent", "--cwd", proj,
                    "--focus", used_by="setup (panorama sends no --workspace: see below)"))
    t2 = result(cli("tab", "tab create (no-focus)", "tab", "create", "--workspace", wid, "--label", "Other",
                    "--cwd", proj, "--no-focus", used_by="setup"))
    tab_a, pane_a = t1["tab"]["tab_id"], t1["root_pane"]["pane_id"]
    tab_b, pane_b = t2["tab"]["tab_id"], t2["root_pane"]["pane_id"]

    # an agent in pane_a (the fake claude reports itself)
    cli("pane", "pane send-text", "pane", "send-text", pane_a, "claude\n", used_by="tests/bench setup")
    time.sleep(2.5)
    cli("pane", "pane report-agent-session", "pane", "report-agent-session", pane_a, "--source", "capture",
        "--agent", "claude", "--agent-session-id", "00000000-0000-4000-8000-000000000001",
        "--session-start-source", "startup", used_by="herdr's Claude Code integration (reference)")
    cli("pane", "pane report-agent", "pane", "report-agent", pane_a, "--source", "capture", "--agent", "claude",
        "--state", "idle", "--agent-session-id", "00000000-0000-4000-8000-000000000001",
        used_by="inbox tools/latency_bench.py, fake_agent.py")

    # reads
    cli("api", "api snapshot", "api", "snapshot", used_by="panorama herdr.snapshot")
    cli("workspace", "workspace list", "workspace", "list", used_by="inbox herdr.all_tabs")
    cli("tab", "tab list --workspace", "tab", "list", "--workspace", wid, used_by="inbox herdr.all_tabs")
    cli("tab", "tab get", "tab", "get", tab_a, used_by="inbox herdr.tab_label")
    cli("pane", "pane list", "pane", "list", used_by="inbox herdr.pane_tabs/focused_pane, panorama tab_root_pane")
    cli("pane", "pane get (agent pane)", "pane", "get", pane_a, used_by="inbox herdr.pane_identity, panorama mcp_server")
    cli("pane", "pane get (shell pane)", "pane", "get", pane_b, used_by="inbox herdr.pane_identity")
    cli("pane", "pane process-info (agent pane)", "pane", "process-info", "--pane", pane_a,
        used_by="panorama herdr.claude_pid/runs_panorama")
    cli("pane", "pane read visible 20", "pane", "read", pane_a, "--source", "visible", "--lines", "20",
        used_by="inbox herdr.panel_focused")
    cli("pane", "pane read visible 30 ansi", "pane", "read", pane_a, "--source", "visible", "--lines", "30",
        "--format", "ansi", used_by="inbox herdr.composer_state")
    cli("agent", "agent list", "agent", "list", used_by="inbox service/alerts")
    cli("agent", "agent get (pane id)", "agent", "get", pane_a, used_by="inbox herdr.live_agent, panorama agent_exists")
    cli("agent", "agent get (shell pane)", "agent", "get", pane_b, used_by="inbox herdr.live_agent")
    cli("plugin", "plugin list --json", "plugin", "list", "--json", used_by="inbox herdr.plugin_root/radar, panorama inbox_link")
    cli("plugin", "plugin config-dir", "plugin", "config-dir", P, used_by="both herdr.plugin_config_dir")
    cli("machine", "machine list --json", "machine", "list", "--json", used_by="panorama machines, inbox remote")

    # writes on panes and tabs
    cli("pane", "pane report-metadata --token", "pane", "report-metadata", pane_a, "--source", "herdr-inbox",
        "--token", "inbox_q=?2", used_by="inbox herdr.set_open_count")
    cli("api", "api snapshot (after token)", "api", "snapshot", used_by="shows the token in the snapshot")
    cli("pane", "pane report-metadata --clear-token", "pane", "report-metadata", pane_a, "--source", "herdr-inbox",
        "--clear-token", "inbox_q", used_by="inbox herdr.set_open_count")
    cli("tab", "tab rename", "tab", "rename", tab_b, "❓ Other", used_by="both herdr.rename_tab")
    cli("tab", "tab focus", "tab", "focus", tab_b, used_by="panorama herdr.focus_tab")
    cli("agent", "agent focus", "agent", "focus", pane_a, used_by="both herdr.focus_agent")
    cli("agent", "agent prompt", "agent", "prompt", pane_a, "capture prompt text", used_by="both herdr.prompt_agent")
    time.sleep(0.5)
    cli("pane", "pane read after prompt", "pane", "read", pane_a, "--source", "visible", "--lines", "20")
    cli("pane", "pane rename", "pane", "rename", pane_b, "capture pane", used_by="panorama herdr.rename_pane")
    cli("pane", "pane zoom --toggle (on)", "pane", "zoom", pane_b, "--toggle", used_by="panorama herdr.zoom_pane")
    cli("pane", "pane zoom --toggle (off)", "pane", "zoom", pane_b, "--toggle", used_by="panorama herdr.zoom_pane")
    cli("notification", "notification show", "notification", "show", "Capture title", "--body", "capture body",
        "--sound", "request", used_by="inbox herdr.notify")
    cli("server", "server reload-config", "server", "reload-config", used_by="inbox radar")

    # plugin panes and actions
    cli("plugin", "plugin action invoke (inbox open)", "plugin", "action", "invoke", f"{I}.open",
        used_by="panorama herdr.invoke_action")
    known = {p["pane_id"] for p in result(cli("pane", "pane list (before popup)", "pane", "list")).get("panes", [])}
    (ISO / "popup-exits").touch()
    cli("plugin", "plugin pane open (popup, default placement)", "plugin", "pane", "open", "--plugin", P,
        "--entrypoint", "panorama", "--cwd", str(ISO / "plugins/panorama-stub"), used_by="panorama herdr.open_view")
    time.sleep(0.5)
    cli("errors", "plugin pane open (second popup while one is open)", "plugin", "pane", "open", "--plugin", I,
        "--entrypoint", "inbox", "--cwd", str(ISO / "plugins/inbox-stub"))

    def close_popup(label):
        """Popups are not in `pane list`; the stub's process exits after 1 s and the popup closes."""
        time.sleep(1.5)
        cli("pane", f"pane list (after the {label} popup exited)", "pane", "list")
    close_popup("panorama")
    (ISO / "popup-exits").unlink()
    cli("plugin", "plugin pane open (inbox popup, env)", "plugin", "pane", "open", "--plugin", I, "--entrypoint", "inbox",
        "--cwd", str(ISO / "plugins/inbox-stub"), "--env", f"INBOX_FOCUS_PANE={pane_a}", used_by="inbox herdr.open_popup")
    time.sleep(0.5)
    close_popup("inbox")
    split = cli("plugin", "plugin pane open (split right of pane, focus)", "plugin", "pane", "open", "--plugin", P,
                "--entrypoint", "panorama", "--cwd", str(ISO / "plugins/panorama-stub"), "--placement", "split",
                "--direction", "right", "--focus", "--target-pane", pane_a, used_by="panorama herdr.open_view(split)")
    split_id = ((result(split).get("plugin_pane") or {}).get("pane") or {}).get("pane_id")
    if split_id:
        cli("plugin", "plugin pane close (split)", "plugin", "pane", "close", split_id)
    home = cli("plugin", "plugin pane open (tab, env, no-focus)", "plugin", "pane", "open", "--plugin", P,
               "--entrypoint", "panorama", "--placement", "tab", "--env", "PANORAMA_HOME=1", "--no-focus",
               "--cwd", str(ISO / "plugins/panorama-stub"), used_by="panorama herdr.open_tab_pane")
    home_pane = ((result(home).get("plugin_pane") or {}).get("pane") or {}).get("pane_id")
    panel = cli("plugin", "plugin pane open (panel split, no-focus)", "plugin", "pane", "open", "--plugin", P,
                "--entrypoint", "panel", "--placement", "split", "--direction", "right", "--target-pane", pane_a,
                "--no-focus", "--cwd", str(ISO / "plugins/panorama-stub"), used_by="panorama herdr.open_panel")
    panel_id = ((result(panel).get("plugin_pane") or {}).get("pane") or {}).get("pane_id")
    time.sleep(0.5)
    if panel_id:
        cli("pane", "pane process-info (panel pane)", "pane", "process-info", "--pane", panel_id,
            used_by="panorama herdr.runs_panorama")

    # raw socket methods
    sock("socket", "pane.layout", {"id": "panorama:1", "method": "pane.layout", "params": {"pane_id": pane_a}},
         used_by="panorama herdr.pane_layout")
    sock("socket", "layout.export", {"id": "panorama:1", "method": "layout.export", "params": {"tab_id": tab_a}},
         used_by="panorama herdr.layout_export")
    sock("socket", "layout.set_split_ratio", {"id": "panorama:1", "method": "layout.set_split_ratio",
                                              "params": {"tab_id": tab_a, "path": [], "ratio": 0.6}},
         used_by="panorama herdr.set_split_ratio")
    sock("socket", "pane.focus", {"id": "panorama:1", "method": "pane.focus", "params": {"pane_id": pane_a}},
         used_by="panorama herdr.focus_pane")
    sock("socket", "tab.move", {"id": "panorama:1", "method": "tab.move", "params": {"tab_id": tab_b, "insert_index": 0}},
         used_by="panorama herdr.move_tab")
    known.update(p["pane_id"] for p in result(cli("pane", "pane list (before new-agent popup)", "pane", "list")).get("panes", []))
    sock("socket", "plugin.pane.open (new-agent popup)", {"id": "panorama:1", "method": "plugin.pane.open", "params": {
        "plugin_id": P, "entrypoint": "new-agent", "placement": "popup", "focus": True, "width": 84, "height": 26,
        "env": {"PANORAMA_NEW_AGENT_CWD": proj}, "cwd": str(ISO / "plugins/panorama-stub")}},
         used_by="panorama herdr.open_new_agent")
    time.sleep(0.5)
    close_popup("new-agent")
    lay = sock("socket", "layout.apply", {"id": "panorama:1", "method": "layout.apply", "params": {
        "tab_label": "New agent", "focus": False, "root": {
            "type": "split", "direction": "right", "ratio": 0.3,
            "first": {"type": "pane", "command": ["panorama", "panel"], "env": {"HERDR_PLUGIN_ID": P},
                      "label": "Panorama panel", "cwd": str(ISO / "plugins/panorama-stub")},
            "second": {"type": "pane", "cwd": proj}}}}, used_by="panorama herdr.apply_panel_layout",
         note="request simplified: panorama sends PANEL_COMMAND (sh -c ...) and up to four HERDR_PLUGIN_* env "
              "values in `first`; the reply shape does not depend on them")
    new_lay = ((lay["replies"] or [{}])[0].get("result") or {}).get("layout") or {}
    new_tab = new_lay.get("tab_id")
    second = ((new_lay.get("root") or {}).get("second") or {}).get("pane_id")

    # agent start in the layout's plain pane, with the fake claude
    if second:
        time.sleep(1.0)
        cli("agent", "agent start (fake claude)", "agent", "start", "capture-agent", "--kind", "claude", "--pane", second,
            "--timeout", "8000", used_by="panorama herdr.start_agent",
            note="the pane runs a stand-in `claude` script, not Claude Code: readiness may not be detected", timeout=20)
        cli("agent", "agent get (by name)", "agent", "get", "capture-agent", used_by="panorama herdr.agent_exists")

    # pane moves and swaps
    if home_pane and new_tab:
        mv = cli("pane", "pane move --tab --split right", "pane", "move", home_pane, "--tab", tab_a, "--split", "right",
                 "--no-focus", "--target-pane", pane_a, used_by="panorama herdr.move_pane_to_tab")
        moved = ((result(mv).get("move_result") or {}).get("pane") or {}).get("pane_id") or home_pane
        cli("pane", "pane swap", "pane", "swap", "--source-pane", moved, "--target-pane", pane_a,
            used_by="panorama herdr.swap_panes")
        cli("pane", "pane move --new-tab", "pane", "move", moved, "--new-tab", "--label", "◆ panorama", "--no-focus",
            "--workspace", wid, used_by="panorama herdr.move_pane_new_tab")

    # the event stream
    def poke():
        cli("events", "tab focus (during subscription)", "tab", "focus", tab_b)
        time.sleep(0.3)
        cli("events", "agent focus (during subscription)", "agent", "focus", pane_a)
    sock("events", "events.subscribe", {"id": "panorama:events", "method": "events.subscribe", "params": {
        "subscriptions": [{"type": "tab.focused"}, {"type": "pane.focused"}]}}, used_by="panorama herdr.events",
         read_for=3.0, until_reply=False, during=poke,
         note="the first reply is the ack; later lines are events triggered by the CLI calls recorded beside it")

    # remote-machine paths (no machines saved: these are the error/empty shapes)
    cli("machine", "--machine unknown api snapshot", "--machine", "nosuch", "api", "snapshot",
        used_by="panorama machines.snapshot", note="no saved machine")
    cli("machine", "--machine unknown agent focus", "--machine", "nosuch", "agent", "focus", pane_a,
        used_by="panorama tui remote focus", note="no saved machine")

    # close paths
    if new_tab:
        cli("tab", "tab close", "tab", "close", new_tab, used_by="panorama herdr.close_tab")

    # panorama's own create_tab argv (no --workspace: herdr picks the workspace), from inside a
    # pane (HERDR_PANE_ID set, as for panorama's processes) and with no pane context
    cli("tab", "tab create (panorama create_tab form, from a pane)", "tab", "create", "--label", "From pane",
        "--focus", "--cwd", proj, env_extra={"HERDR_PANE_ID": pane_b}, used_by="panorama herdr.create_tab")
    cli("tab", "tab create (panorama create_tab form, no pane context)", "tab", "create", "--label", "No context",
        "--no-focus", "--cwd", proj, used_by="panorama herdr.create_tab")

    # error replies
    E = "errors"
    cli(E, "pane get (unknown)", "pane", "get", "w9-p99")
    cli(E, "pane get (malformed id)", "pane", "get", "not-a-pane")
    cli(E, "tab get (unknown)", "tab", "get", "w9-t99")
    cli(E, "tab rename (unknown)", "tab", "rename", "w9-t99", "x")
    cli(E, "tab focus (unknown)", "tab", "focus", "w9-t99")
    cli(E, "tab close (unknown)", "tab", "close", "w9-t99")
    cli(E, "tab list (unknown workspace)", "tab", "list", "--workspace", "w99")
    cli(E, "agent get (no agent)", "agent", "get", "nobody")
    cli(E, "agent focus (shell pane)", "agent", "focus", pane_b)
    cli(E, "agent prompt (shell pane)", "agent", "prompt", pane_b, "hello")
    cli(E, "agent start (unknown pane)", "agent", "start", "x", "--kind", "claude", "--pane", "w9-p99", "--timeout", "1000")
    cli(E, "pane process-info (unknown)", "pane", "process-info", "--pane", "w9-p99")
    cli(E, "pane read (unknown)", "pane", "read", "w9-p99", "--source", "visible", "--lines", "20")
    cli(E, "pane report-metadata (unknown)", "pane", "report-metadata", "w9-p99", "--source", "herdr-inbox",
        "--token", "inbox_q=?1")
    cli(E, "pane move (unknown tab)", "pane", "move", pane_b, "--tab", "w9-t99", "--split", "right", "--no-focus")
    cli(E, "pane swap (across tabs: ok, changed=false)", "pane", "swap", "--source-pane", pane_a, "--target-pane", pane_b)
    cli(E, "pane zoom (unknown)", "pane", "zoom", "w9-p99", "--toggle")
    cli(E, "pane rename (unknown)", "pane", "rename", "w9-p99", "x")
    cli(E, "plugin pane open (unknown plugin)", "plugin", "pane", "open", "--plugin", "nobody.none", "--entrypoint", "x")
    cli(E, "plugin pane open (unknown entrypoint)", "plugin", "pane", "open", "--plugin", P, "--entrypoint", "nope")
    cli(E, "plugin pane close (not a plugin pane)", "plugin", "pane", "close", pane_b)
    cli(E, "plugin action invoke (unknown)", "plugin", "action", "invoke", "nobody.none.open")
    cli(E, "plugin config-dir (unknown)", "plugin", "config-dir", "nobody.none")
    cli(E, "unknown subcommand", "pane", "frobnicate")
    sock(E, "socket: unknown method", {"id": "panorama:1", "method": "no.such.method", "params": {}})
    sock(E, "socket: invalid params", {"id": "panorama:1", "method": "tab.move", "params": {"tab_id": tab_a}})
    sock(E, "socket: unknown tab", {"id": "panorama:1", "method": "tab.move", "params": {"tab_id": "w9-t99", "insert_index": 0}})
    sock(E, "socket: unknown pane (pane.layout)", {"id": "panorama:1", "method": "pane.layout", "params": {"pane_id": "w9-p99"}})
    sock(E, "socket: unknown pane (pane.focus)", {"id": "panorama:1", "method": "pane.focus", "params": {"pane_id": "w9-p99"}})
    sock(E, "socket: unknown tab (layout.export)", {"id": "panorama:1", "method": "layout.export", "params": {"tab_id": "w9-t99"}})
    sock(E, "socket: bad split path", {"id": "panorama:1", "method": "layout.set_split_ratio",
                                      "params": {"tab_id": tab_b, "path": [True, True, True], "ratio": 0.5}})
    sock(E, "socket: plugin.pane.open unknown plugin", {"id": "panorama:1", "method": "plugin.pane.open",
                                                         "params": {"plugin_id": "nobody.none", "entrypoint": "x"}})
    sock(E, "socket: events.subscribe unknown type", {"id": "panorama:events", "method": "events.subscribe",
                                                      "params": {"subscriptions": [{"type": "no.such.event"}]}})
    sock(E, "socket: malformed JSON", "{not json\n")
    sock(E, "socket: missing id", {"method": "pane.focus", "params": {"pane_id": pane_a}})
    sock(E, "socket: missing params", {"id": "panorama:1", "method": "pane.focus"})


if __name__ == "__main__":
    main()
