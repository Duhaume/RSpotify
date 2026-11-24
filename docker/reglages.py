#!/usr/bin/env python3
"""Settings page served by the Pi itself (port 8080 by default).

It edits the files in /data (raspotify.conf, hotspot.env, .env), then restarts
the containers through the Docker socket. Python standard library only.
"""
import base64
import hmac
import html
import http.client
import json
import os
import re
import shlex
import socket
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DATA = os.environ.get("REGLAGES_DATA", "/data")
PORT = int(os.environ.get("REGLAGES_PORT", "8080") or 8080)
CODE = os.environ.get("REGLAGES_CODE", "")
CONF = os.path.join(DATA, "raspotify.conf")
HOTSPOT = os.path.join(DATA, "hotspot.env")
DOTENV = os.path.join(DATA, ".env")
SPOTIFY = "raspotify"
HOTSPOT_CT = "raspotify-hotspot"

DEVICE_TYPES = [
    ("speaker", "Enceinte"), ("computer", "Ordinateur"), ("tablet", "Tablette"),
    ("smartphone", "Téléphone"), ("tv", "Télévision"), ("avr", "Ampli audio/vidéo"),
    ("stb", "Décodeur TV"), ("audiodongle", "Clé audio"), ("gameconsole", "Console de jeu"),
    ("castaudio", "Diffusion audio"), ("castvideo", "Diffusion vidéo"), ("automobile", "Voiture"),
    ("smartwatch", "Montre"), ("chromebook", "Chromebook"), ("carthing", "Accessoire voiture"),
    ("homething", "Appareil maison"),
]
BITRATES = [("96", "Normale (96)"), ("160", "Haute (160)"), ("320", "Très haute (320)")]
CHANNELS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 36, 40, 44, 48]


# --- Files ------------------------------------------------------------------
def read_lines(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read().splitlines()
    except FileNotFoundError:
        return []


def write_lines(path, lines, mode=0o600):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    os.chmod(tmp, mode)
    os.replace(tmp, path)


def unquote(v):
    try:
        parts = shlex.split(v, posix=True)
        return " ".join(parts)
    except ValueError:
        return v.strip().strip("'\"")


def get_key(lines, key):
    """Value of an active KEY=value line, or None when absent or commented."""
    for line in lines:
        m = re.match(r"^\s*" + re.escape(key) + r"=(.*)$", line)
        if m:
            return unquote(m.group(1))
    return None


def set_key(lines, key, value, quote=False):
    """Make KEY active with value, or comment it out when value is None."""
    if value is None:
        new = None
    else:
        v = shlex.quote(value) if quote and value != "" else value
        new = f"{key}={v}"
    active = re.compile(r"^\s*" + re.escape(key) + r"=")
    commented = re.compile(r"^#" + re.escape(key) + r"=")  # e.g. "#LIBRESPOT_BITRATE=160", not prose
    found = False
    for i, line in enumerate(lines):
        if active.match(line):
            # Rewrite the first active line, comment out any later duplicate.
            if new is not None and not found:
                lines[i] = new
            else:
                lines[i] = "#" + line.lstrip()
            found = True
    if found or new is None:
        return lines
    for i, line in enumerate(lines):
        if commented.match(line):
            lines[i] = new
            return lines
    lines.append(new)
    return lines


def load():
    c = read_lines(CONF)
    h = read_lines(HOTSPOT)
    return {
        "name": get_key(c, "LIBRESPOT_NAME") or "",
        "device_type": get_key(c, "LIBRESPOT_DEVICE_TYPE") or "speaker",
        "bitrate": get_key(c, "LIBRESPOT_BITRATE") or "160",
        "initial_volume": get_key(c, "LIBRESPOT_INITIAL_VOLUME") or "",
        "normalisation": get_key(c, "LIBRESPOT_ENABLE_VOLUME_NORMALISATION") is not None,
        "hotspot": (get_key(h, "AP_ENABLED") or "no") == "yes",
        "ssid": get_key(h, "AP_SSID") or "Raspotify",
        "password": get_key(h, "AP_PASSWORD") or "",
        "channel": get_key(h, "AP_CHANNEL") or "6",
        "country": get_key(h, "AP_COUNTRY") or "FR",
        "interface": get_key(h, "AP_INTERFACE") or "wlan0",
        "share": (get_key(h, "AP_SHARE_INTERNET") or "yes") == "yes",
        "uplink": get_key(h, "AP_UPLINK") or "eth0",
    }


def validate(s):
    errs = []
    if any(ch in s["name"] for ch in "\"\n\r"):
        errs.append("Le nom de l'enceinte ne peut pas contenir de guillemets.")
    if s["device_type"] not in [d for d, _ in DEVICE_TYPES]:
        errs.append("Type d'appareil inconnu.")
    if s["bitrate"] not in [b for b, _ in BITRATES]:
        errs.append("Qualité inconnue.")
    if s["initial_volume"] and not (s["initial_volume"].isdigit() and 0 <= int(s["initial_volume"]) <= 100):
        errs.append("Le volume de départ est un nombre de 0 à 100.")
    if s["hotspot"]:
        n = len(s["ssid"].encode())
        if not 1 <= n <= 32:
            errs.append("Le nom du réseau Wi-Fi doit faire de 1 à 32 caractères.")
        if s["password"] and not 8 <= len(s["password"]) <= 63:
            errs.append("Le mot de passe Wi-Fi doit faire de 8 à 63 caractères.")
        if not s["channel"].isdigit() or int(s["channel"]) not in CHANNELS:
            errs.append("Canal Wi-Fi invalide.")
        if not re.fullmatch(r"[A-Za-z]{2}", s["country"]):
            errs.append("Le pays est un code de 2 lettres (FR, BE, CH...).")
        if not re.fullmatch(r"[A-Za-z0-9_.-]{1,15}", s["interface"]):
            errs.append("Nom de carte Wi-Fi invalide.")
        if s["share"] and s["uplink"] == s["interface"]:
            errs.append("Le Pi ne peut pas partager internet par la carte qui crée le réseau : utilisez eth0 ou une 2e clé Wi-Fi.")
    for v in (s["ssid"], s["password"]):
        if "\n" in v or "\r" in v:
            errs.append("Pas de retour à la ligne dans les champs Wi-Fi.")
    return errs


def save(s):
    c = read_lines(CONF)
    set_key(c, "LIBRESPOT_NAME", s["name"] or None, quote=True)
    set_key(c, "LIBRESPOT_DEVICE_TYPE", None if s["device_type"] == "speaker" else s["device_type"])
    set_key(c, "LIBRESPOT_BITRATE", None if s["bitrate"] == "160" else s["bitrate"])
    set_key(c, "LIBRESPOT_INITIAL_VOLUME", s["initial_volume"] or None)
    set_key(c, "LIBRESPOT_ENABLE_VOLUME_NORMALISATION", "" if s["normalisation"] else None)
    write_lines(CONF, c)

    h = read_lines(HOTSPOT)
    set_key(h, "AP_ENABLED", "yes" if s["hotspot"] else "no")
    set_key(h, "AP_SSID", s["ssid"], quote=True)
    set_key(h, "AP_PASSWORD", s["password"], quote=True)
    set_key(h, "AP_CHANNEL", s["channel"])
    set_key(h, "AP_COUNTRY", s["country"].upper())
    set_key(h, "AP_INTERFACE", s["interface"])
    set_key(h, "AP_SHARE_INTERNET", "yes" if s["share"] else "no")
    set_key(h, "AP_UPLINK", s["uplink"])
    write_lines(HOTSPOT, h)


# --- Docker Engine API over the Unix socket ------------------------------------
class UnixHTTPConnection(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost", timeout=30)
        self.sock_path = path

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(30)
        self.sock.connect(self.sock_path)


def docker(method, path):
    conn = UnixHTTPConnection("/var/run/docker.sock")
    try:
        conn.request(method, path, headers={"Content-Type": "application/json"})
        r = conn.getresponse()
        return r.status, r.read()
    except OSError as e:
        return 0, str(e).encode()
    finally:
        conn.close()


def state(name):
    status, body = docker("GET", f"/containers/{name}/json")
    if status != 200:
        return "absent"
    try:
        return json.loads(body)["State"]["Status"]
    except (ValueError, KeyError):
        return "inconnu"


def logs(name, tail=120):
    status, body = docker("GET", f"/containers/{name}/logs?stdout=1&stderr=1&tail={tail}")
    if status != 200:
        return "(pas de journal)"
    out, i = [], 0
    while i + 8 <= len(body) and body[i] in (0, 1, 2):
        size = int.from_bytes(body[i + 4:i + 8], "big")
        out.append(body[i + 8:i + 8 + size])
        i += 8 + size
    raw = b"".join(out) if out else body
    return raw.decode("utf-8", "replace")


def apply(s):
    """Restart what reads the files. Returns (done, problems)."""
    done, problems = [], []
    st, _ = docker("POST", f"/containers/{SPOTIFY}/restart?t=5")
    if st == 204:
        done.append("Lecteur redémarré.")
    else:
        problems.append("Le lecteur n'a pas pu redémarrer (lancez « sudo ./install.sh » sur le Pi).")
    if state(HOTSPOT_CT) == "absent":
        problems.append("Point d'accès pas encore installé : lancez « sudo ./install.sh » une fois.")
    else:
        # The container re-reads hotspot.env at start: on, it takes the Wi-Fi card; off, it idles.
        st, _ = docker("POST", f"/containers/{HOTSPOT_CT}/restart?t=10")
        if st != 204:
            problems.append("Le point d'accès n'a pas pu redémarrer.")
        else:
            done.append("Point d'accès activé." if s["hotspot"] else "Point d'accès désactivé.")
    return done, problems


# --- Page ---------------------------------------------------------------------
CSS = """
*{box-sizing:border-box}body{margin:0;font-family:system-ui,sans-serif;background:#121418;color:#dce0e6}
header{background:#1a1d23;padding:16px 20px;display:flex;align-items:center;gap:12px}
.logo{width:34px;height:34px;border-radius:50%;background:#1c5c42;display:grid;place-items:center}
.logo i{width:20px;height:20px;border-radius:50%;border:3px solid #3ddc97;display:block}
h1{font-size:19px;margin:0;color:#fff}header p{margin:2px 0 0;color:#969eaa;font-size:13px}
main{max-width:760px;margin:0 auto;padding:16px}
.card{background:#22262e;border-radius:10px;padding:16px;margin-bottom:14px}
h2{font-size:15px;margin:0 0 12px;color:#fff}
label{display:block;margin:10px 0 4px;font-size:14px}
input[type=text],input[type=password],input[type=number],select{width:100%;padding:9px 10px;border-radius:6px;border:1px solid #3a404c;background:#0e1013;color:#fff;font-size:15px}
.row{display:flex;gap:10px}.row>*{flex:1}
.switch{display:flex;align-items:center;gap:10px;margin:10px 0}.switch input{width:20px;height:20px;accent-color:#3ddc97}
button{background:#1c5c42;color:#fff;border:1px solid #3ddc97;border-radius:8px;padding:11px 18px;font-size:15px;cursor:pointer}
button.alt{background:#2c313b;border-color:#464c58}
.pill{display:inline-block;padding:3px 10px;border-radius:12px;border:1px solid #969eaa;color:#969eaa;font-size:13px;margin-right:6px}
.pill.on{border-color:#3ddc97;color:#3ddc97}.pill.off{border-color:#f0645a;color:#f0645a}
.msg{padding:10px 12px;border-radius:8px;margin-bottom:12px}.ok{background:#1c5c42}.err{background:#5a2420}
.hint{color:#969eaa;font-size:12.5px;margin:4px 0}
pre{background:#0e1013;padding:10px;border-radius:8px;max-height:320px;overflow:auto;font-size:12px;white-space:pre-wrap}
"""


def esc(v):
    return html.escape(str(v), quote=True)


def options(items, current):
    return "".join(
        f'<option value="{esc(v)}"{" selected" if str(v) == str(current) else ""}>{esc(t)}</option>' for v, t in items
    )


def pill(name, label):
    st = state(name)
    cls = "on" if st == "running" else ("off" if st in ("exited", "dead", "restarting") else "")
    txt = {"running": "en marche", "exited": "arrêté", "absent": "non installé", "restarting": "redémarre"}.get(st, st)
    return f'<span class="pill {cls}">{esc(label)} : {esc(txt)}</span>'


def page(s, msgs=(), errs=()):
    host = socket.gethostname()
    name_ph = f"raspotify ({host})"
    banner = "".join(f'<div class="msg ok">{esc(m)}</div>' for m in msgs)
    banner += "".join(f'<div class="msg err">{esc(e)}</div>' for e in errs)
    chk = lambda b: " checked" if b else ""
    chan = options([(c, f"{c} ({'5 GHz' if c > 14 else '2,4 GHz'})") for c in CHANNELS], s["channel"])
    return f"""<!doctype html><html lang="fr"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Raspotify — réglages</title>
<style>{CSS}</style></head><body>
<header><div class="logo"><i></i></div><div><h1>Raspotify Docker</h1>
<p>Réglages servis par le Pi « {esc(host)} »</p></div></header><main>
{banner}
<div class="card"><h2>État</h2>{pill(SPOTIFY, "Lecteur")}{pill(HOTSPOT_CT, "Point d'accès")}
<p class="hint">Tout tourne sur ce Pi : le lecteur, le point d'accès et cette page.</p></div>
<form method="post" action="/save">
<div class="card"><h2>Enceinte</h2>
<label>Nom dans Spotify</label><input type="text" name="name" value="{esc(s['name'])}" placeholder="{esc(name_ph)}">
<div class="row"><div><label>Type d'appareil</label><select name="device_type">{options(DEVICE_TYPES, s['device_type'])}</select></div>
<div><label>Qualité (kbit/s)</label><select name="bitrate">{options(BITRATES, s['bitrate'])}</select></div></div>
<label>Volume de départ (%, vide = 50)</label><input type="number" min="0" max="100" name="initial_volume" value="{esc(s['initial_volume'])}">
<div class="switch"><input type="checkbox" name="normalisation" id="n"{chk(s['normalisation'])}><label for="n" style="margin:0">Même volume pour tous les titres</label></div>
</div>
<div class="card"><h2>Point d'accès Wi-Fi</h2>
<div class="switch"><input type="checkbox" name="hotspot" id="h"{chk(s['hotspot'])}><label for="h" style="margin:0">Le Pi crée son propre réseau Wi-Fi</label></div>
<p class="hint">Spotify a besoin d'internet : reliez le Pi par câble (ou une 2e clé Wi-Fi). Si vous êtes connecté à cette page par le Wi-Fi que vous changez, la connexion sera coupée.</p>
<div class="row"><div><label>Nom du réseau</label><input type="text" name="ssid" value="{esc(s['ssid'])}" maxlength="32"></div>
<div><label>Mot de passe (8 caractères min., vide = ouvert)</label><input type="password" name="password" value="{esc(s['password'])}" maxlength="63"></div></div>
<div class="row"><div><label>Canal</label><select name="channel">{chan}</select></div>
<div><label>Pays</label><input type="text" name="country" value="{esc(s['country'])}" maxlength="2"></div></div>
<div class="row"><div><label>Carte Wi-Fi</label><input type="text" name="interface" value="{esc(s['interface'])}"></div>
<div><label>Internet du Pi via</label><input type="text" name="uplink" value="{esc(s['uplink'])}"></div></div>
<div class="switch"><input type="checkbox" name="share" id="s"{chk(s['share'])}><label for="s" style="margin:0">Partager internet avec les téléphones</label></div>
</div>
<button type="submit">Enregistrer et appliquer</button>
</form>
<div class="card" style="margin-top:14px"><h2>Journal</h2>
<form method="get" action="/" class="row" style="margin-bottom:8px"><button class="alt" name="log" value="{SPOTIFY}">Lecteur</button><button class="alt" name="log" value="{HOTSPOT_CT}">Point d'accès</button></form>
{s.get('_log', '')}</div>
</main></body></html>"""


class Handler(BaseHTTPRequestHandler):
    server_version = "raspotify-reglages"

    def log_message(self, fmt, *args):
        print("[reglages] " + (fmt % args), flush=True)

    def authorized(self):
        if not CODE:
            return True
        h = self.headers.get("Authorization", "")
        if h.startswith("Basic "):
            try:
                _, _, pw = base64.b64decode(h[6:]).decode().partition(":")
                if hmac.compare_digest(pw, CODE):
                    return True
            except ValueError:
                pass
        self.send_response(401)
        self.send_header("WWW-Authenticate", 'Basic realm="Raspotify (code d\'acces)"')
        self.end_headers()
        return False

    def send_html(self, body, status=200):
        data = body.encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if not self.authorized():
            return
        url = urllib.parse.urlparse(self.path)
        if url.path != "/":
            self.send_error(404)
            return
        q = urllib.parse.parse_qs(url.query)
        s = load()
        msgs = ["Réglages enregistrés et appliqués."] if q.get("ok") else []
        which = (q.get("log") or [""])[0]
        if which in (SPOTIFY, HOTSPOT_CT):
            s["_log"] = f"<pre>{esc(logs(which))}</pre>"
        self.send_html(page(s, msgs))

    def do_POST(self):
        if not self.authorized():
            return
        if self.path != "/save":
            self.send_error(404)
            return
        n = min(int(self.headers.get("Content-Length", "0") or 0), 64 * 1024)
        form = urllib.parse.parse_qs(self.rfile.read(n).decode("utf-8", "replace"), keep_blank_values=True)
        f = lambda k, d="": (form.get(k) or [d])[0].strip()
        s = {
            "name": f("name"), "device_type": f("device_type", "speaker"), "bitrate": f("bitrate", "160"),
            "initial_volume": f("initial_volume"), "normalisation": "normalisation" in form,
            "hotspot": "hotspot" in form, "ssid": f("ssid"), "password": (form.get("password") or [""])[0],
            "channel": f("channel", "6"), "country": f("country", "FR"), "interface": f("interface", "wlan0"),
            "share": "share" in form, "uplink": f("uplink", "eth0"),
        }
        errs = validate(s)
        if errs:
            self.send_html(page(s, errs=errs), 400)
            return
        save(s)
        done, problems = apply(s)
        self.send_html(page(load(), msgs=["Réglages enregistrés."] + done, errs=problems))


if __name__ == "__main__":
    print(f"[reglages] settings page on port {PORT}" + (" (access code required)" if CODE else ""), flush=True)
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
