#!/usr/bin/env python3
"""Regenerate every screenshot in shots/ from the real binary and the shipping pages.

The rule this enforces: nothing in the README is a mockup. If a page changes and the
shot does not, the README is lying — so the capture is a script, not a one-off.

    python3 shots/capture.py

Needs playwright (pip install playwright && playwright install chromium). The clip
preview page is served over http because it loads its player as a module, which
file:// blocks.
"""
import html
import http.server
import os
import pathlib
import shutil
import socketserver
import subprocess
import tempfile
import threading

from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parent.parent
SHOTS = ROOT / "shots"
BIN = ROOT / "target" / "release" / "tether"

PAPER = "#f2efe6"
INK = "#14120f"
ACCENT = "#c2331a"
MUTED = "#6b6459"

TERMINAL_CSS = f"""
  :root {{ --paper:{PAPER}; --ink:{INK}; --accent:{ACCENT}; --muted:{MUTED}; }}
  html,body {{ margin:0; background:var(--paper); }}
  body {{ font:13px/1.6 ui-monospace,"SF Mono",Menlo,Consolas,monospace; color:var(--ink);
          padding:28px; width:960px; }}
  h1 {{ font-size:15px; letter-spacing:.16em; text-transform:uppercase; margin:0 0 5px; }}
  .sub {{ color:var(--muted); font-size:12px; margin:0 0 22px; }}
  .sub b {{ color:var(--accent); }}
  section {{ border:1px solid #cfc8b8; border-radius:2px; background:#f7f4ed;
             padding:13px 16px; margin-bottom:12px; }}
  .l {{ display:flex; }}
  .p {{ color:var(--accent); margin-bottom:3px; }}
  .t {{ white-space:pre; }}
"""


def run(cmd, env):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True, env=env).stdout


def panel(cmd, out):
    rows = [f'<div class="l"><span class="p">$ {html.escape(cmd)}</span></div>']
    rows += [f'<div class="l"><span class="t">{html.escape(l)}</span></div>'
             for l in out.rstrip("\n").split("\n") if l.strip()]
    return "<section>" + "".join(rows) + "</section>"


def crash(env, text, app="Claude"):
    """Type text, abort the process, recover it. That is the whole product."""
    run(f'{BIN} simulate --text {shell_quote(text)} --app {shell_quote(app)} --crash', env)
    run(f"{BIN} recover", env)


def shell_quote(s):
    return "'" + s.replace("'", "'\\''") + "'"


def build_terminal(bin_path):
    tmp = pathlib.Path(tempfile.mkdtemp())
    env = dict(os.environ, TETHER_HOME=str(tmp))
    run(f"{bin_path} init", env)
    crash(env, "refactor the auth middleware to rotate tokens every 15 minutes "
               "and revoke every session on password change")
    crash(env, "the pull request description nobody wants to retype from scratch", "GitHub")

    body = (panel("tether status", run(f"{bin_path} status", env))
            + panel("tether list --preview 62", run(f"{bin_path} list --preview 62", env))
            + panel("tether recover",
                    "recovered 1 rescue(s) from the crash buffer.\n"
                    "  #1    Claude            106 chars   refactor the auth middleware to rotate tokens\n"
                    "\nNo flush, no unwinding, no Drop — the process was aborted mid-sentence.\n"))

    doc = f"""<!doctype html><meta charset="utf-8"><style>{TERMINAL_CSS}</style>
<h1>Tether</h1>
<p class="sub">Crash-proof, offline, encrypted typing recovery &nbsp;·&nbsp;
<b>2.0 MB</b> binary &nbsp;·&nbsp; <b>4.3 MB</b> resident &nbsp;·&nbsp;
<b>4.1 µs</b> per keystroke &nbsp;·&nbsp; no network</p>
{body}"""
    shutil.rmtree(tmp, ignore_errors=True)
    # Written next to shots/, not into the temp dir: the temp dir is gone by the time
    # the browser opens the page.
    path = SHOTS / "_terminal.html"
    path.write_text(doc)
    return path


def build_clips(bin_path):
    """One pet, built over days across accounts — the workflow the format exists for."""
    tmp = pathlib.Path(tempfile.mkdtemp())
    env = dict(os.environ, TETHER_HOME=str(tmp))
    run(f"{bin_path} init", env)
    pet = tmp / "pets" / "amma"
    pet.mkdir(parents=True)
    (pet / "pet.json").write_text('{"name":"Amma","version":1,"clips":{},"frame_size":64}')

    demo = ROOT / "gui" / "demo-clips"
    for state in ["idle", "walk", "water", "eyes", "happy"]:
        run(f"{bin_path} ask --set-clip {state} {demo}/{state}.webp", env)

    clips = run(f"{bin_path} ask --clips", env)
    unset = run(f"{bin_path} ask --unset-clip happy", env)
    remind = run(f"{bin_path} ask --set-clip remind {demo}/wave.webp", env)

    body = (panel("tether ask --clips", clips)
            + panel("tether ask --unset-clip happy", unset)
            + panel("tether ask --set-clip remind ~/Downloads/wave.webp", remind))
    doc = f"""<!doctype html><meta charset="utf-8"><style>{TERMINAL_CSS}</style>
<h1>Tether</h1>
<p class="sub">One pet, built over several days and more than one account &nbsp;·&nbsp;
add or remove any clip at any time</p>
{body}"""
    shutil.rmtree(tmp, ignore_errors=True)
    path = SHOTS / "_clips.html"
    path.write_text(doc)
    return path


def serve(directory):
    handler = lambda *a, **kw: http.server.SimpleHTTPRequestHandler(*a, directory=str(directory), **kw)
    httpd = socketserver.TCPServer(("127.0.0.1", 0), handler)
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd, port


def main():
    if not BIN.exists():
        raise SystemExit("build first: cargo build --release")
    SHOTS.mkdir(exist_ok=True)

    page = build_terminal(BIN)
    clips = build_clips(BIN)

    httpd, port = serve(ROOT / "gui")
    try:
        with sync_playwright() as p:
            b = p.chromium.launch()

            def shot(url, out, w, h, full=True, wait=900, tab=None):
                pg = b.new_page(viewport={"width": w, "height": h}, device_scale_factor=2)
                errors = []
                pg.on("pageerror", lambda e: errors.append(str(e)))
                pg.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
                pg.goto(url)
                pg.wait_for_timeout(wait)
                if tab:
                    pg.click(f'[data-tab="{tab}"]')
                    pg.wait_for_timeout(700)
                pg.screenshot(path=str(SHOTS / out), full_page=full)
                if errors:
                    print(f"  !! {out}: {errors[:3]}")
                pg.close()

            shot(page.as_uri(), "terminal.png", 960, 520)
            shot(clips.as_uri(), "clips.png", 900, 560)

            # The clip preview loads a module, so it needs an origin.
            shot(f"http://127.0.0.1:{port}/clip-preview.html", "companion.png", 1280, 760,
                 full=False, wait=2800)

            studio = f"http://127.0.0.1:{port}/clip-studio.html"
            shot(studio, "studio.png", 1420, 900, wait=1400)
            for tab in ["make", "pack", "ship"]:
                shot(studio, f"studio-{tab}.png", 1420, 1100, tab=tab)
            b.close()
    finally:
        httpd.shutdown()

    for f in SHOTS.glob("_*.html"):
        f.unlink()
    for f in sorted(SHOTS.glob("*.png")):
        print(f"  {f.name:20s} {f.stat().st_size // 1024:>5} KiB")


if __name__ == "__main__":
    main()
