# Mechanics sheet — raspotify 0.48.4 (librespot 0.8.0), arm64 package

Sources: the package's own files (`/etc/raspotify/conf`, `raspotify.service`,
`raspotify-crash-report-generator.sh`, `postinst`) and the `librespot` program
(Ghidra pseudo-code and its stored strings). Addresses are in the built-in
engine's numbering (Ghidra adds 0x100000).

## How settings reach librespot
- `get_setup::{closure#0}::{closure#1}` at **0xd8a84**: for every environment
  variable, strip the prefix `LIBRESPOT_`, replace `_` with `-`, lower-case it →
  the command-line option name. So `LIBRESPOT_FOO_BAR` = `--foo-bar`.
- Flags are on when the variable exists (empty value); `off` would still count
  as on (config comment), so the rebuild writes flags as `LIBRESPOT_X=` or
  comments them out.
- systemd unit: `LIBRESPOT_NAME="%N (%H)"` → **"raspotify (<hostname>)"**,
  `LIBRESPOT_BACKEND=alsa`, `LIBRESPOT_CACHE=/var/cache/raspotify`,
  `LIBRESPOT_SYSTEM_CACHE=/var/lib/raspotify`, `TMPDIR=/run/raspotify` (RAM),
  then `EnvironmentFile=-/etc/raspotify/conf` overrides those.
  Rebuild: `entrypoint-spotify.sh` sets the same defaults, then sources the
  mounted conf.

## Raspotify defaults (active lines of /etc/raspotify/conf)
| Setting | Value |
|---|---|
| LIBRESPOT_DISABLE_AUDIO_CACHE | on |
| LIBRESPOT_DISABLE_CREDENTIAL_CACHE | on |
| LIBRESPOT_ENABLE_VOLUME_NORMALISATION | on |
| LIBRESPOT_QUIET | on |
| TMPDIR | /tmp (on disk; comment out for RAM) |

## librespot defaults (help text strings at 0x9861d0, used by get_setup 0xdf338)
| Option | Default | Range |
|---|---|---|
| bitrate | 160 | 96 / 160 / 320 |
| format | S16 | F64 F32 S32 S24 S24_3 S16 |
| dither | tpdf for S16/S24/S24_3, none otherwise | none gpdf tpdf tpdf_hp |
| device-type | speaker | 16 types |
| mixer | softvol | softvol / alsa |
| alsa-mixer-control | PCM | |
| initial-volume | 50 (softvol) | 0–100 % |
| volume-ctrl | log | cubic fixed linear log |
| volume-range | 60.0 dB (softvol) | 0.0–100.0 |
| volume-steps | 64 | (not exposed in the rebuild) |
| normalisation-method | dynamic | basic / dynamic |
| normalisation-gain-type | auto | track album auto |
| normalisation-pregain | 0.0 dB | -10.0–10.0 |
| normalisation-threshold | -2.0 dBFS | 0.0 – -10.0 |
| normalisation-attack | 5 ms | 1–500 |
| normalisation-release | 100 ms | 1–1000 |
| normalisation-knee | 5.0 dB | 0.0–10.0 (the conf comment says 1.0; the binary's help says 5.0, which the rebuild follows) |
| autoplay | follows the client | on / off |
| zeroconf-port | random | 1–65535 |

## Discovery
- Zeroconf through Avahi (string "Failed to connect to Avahi, zeroconf discovery
  will not work until avahi-daemon is started", function 0x32dc64). The package
  depends on `avahi-daemon`. Rebuild: the image runs its own dbus + avahi
  (`AVAHI_MODE=container`) or uses the Pi's (`host`, mounting /run/dbus);
  host networking so mDNS reaches phones. Tested: "Published zeroconf service".

## Restart rule (raspotify-crash-report-generator.sh)
- On failure: write a crash report (system info, config with USERNAME,
  PASSWORD, ACCESS_TOKEN, PROXY masked as XXXXXXXX, `aplay -l`, `aplay -L`,
  `librespot -d ?`), file mode 600.
- Count failures in the last **120 s**; if fewer than **6**, wait **10 s** and
  restart; otherwise stay stopped.
- Rebuild: the same loop in `entrypoint-spotify.sh`; report in
  `/var/lib/raspotify/crash_report`.

## Install / remove (postinst, postrm)
- postinst: config dir chmod 600 root:root, enable + start the service.
- postrm purge: delete config, caches, overrides.
- Rebuild: `install.sh` (install/stop/logs/remove), conf files chmod 600,
  `restart: unless-stopped`; `remove` keeps the cache volumes.

## Added: everything on the Pi, settings over http (not in the original)
Three Docker containers on the Pi, all with host networking: `raspotify`
(player), `raspotify-hotspot` (idle unless `AP_ENABLED=yes` in hotspot.env) and
`raspotify-reglages` (Python settings page, port 8080, HTTP basic auth with a
6-digit code made by install.sh). The page edits raspotify.conf and
hotspot.env, then restarts the two other containers through the Docker socket.
Open it from any browser: `http://<pi>.local:8080`, or `http://192.168.50.1:8080`
on the Pi's own Wi-Fi. All values here are the rebuild's choices ("estimated").

## Added: Wi-Fi access point (not in the original)
All values below are the rebuild's choices ("estimated": nothing in the
original covers this).
| Setting | Default |
|---|---|
| interface | wlan0 |
| SSID | Raspotify |
| password | empty = open network; 8–63 characters otherwise (WPA2-PSK, CCMP) |
| channel | 6 (2.4 GHz); 36–48 = 5 GHz |
| country | FR |
| Pi address | 192.168.50.1/24 |
| DHCP range | 192.168.50.10 – 192.168.50.100, lease 12 h |
| share internet | yes, NAT to eth0 |
- NetworkManager is told to leave the interface alone
  (`/etc/NetworkManager/conf.d/99-raspotify-hotspot.conf`); `remove` gives it back.
