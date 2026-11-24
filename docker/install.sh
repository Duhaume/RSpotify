#!/bin/sh
# Installs and starts Raspotify in Docker on this Raspberry Pi. Everything runs
# on the Pi: the speaker, the optional Wi-Fi access point and the settings page.
#   sudo ./install.sh           install / update and start
#   sudo ./install.sh stop      stop the containers
#   sudo ./install.sh remove    stop and remove the containers
#   sudo ./install.sh logs      follow the logs
#   sudo ./install.sh images    install / start with images already loaded
#                               (from build-images.sh --pi): nothing is compiled on the Pi
set -e

cd "$(dirname "$0")"

if [ "$(id -u)" -ne 0 ]; then
    exec sudo sh "$0" "$@"
fi

say() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$*"; }

envval() { sed -n "s/^$1=//p" "$2" 2>/dev/null | head -n1 | sed "s/^'\(.*\)'$/\1/"; }

FILE=docker-compose.yml
BUILD=--build
if [ "${1:-}" = "images" ]; then
    FILE=docker-compose.images.yml
    BUILD=
    set -- install
fi

compose() {
    if docker compose version >/dev/null 2>&1; then
        docker compose -f "$FILE" "$@"
    else
        docker-compose -f "$FILE" "$@"
    fi
}

# Older versions handed the Wi-Fi card over permanently; undo that.
NM_CONF=/etc/NetworkManager/conf.d/99-raspotify-hotspot.conf
if [ -f "$NM_CONF" ]; then
    rm -f "$NM_CONF"
    nmcli general reload conf 2>/dev/null || true
fi

case "${1:-install}" in
    stop)
        compose stop
        exit 0 ;;
    logs)
        compose logs -f --tail=100
        exit 0 ;;
    remove)
        compose down
        say "removed (the cache volumes are kept; 'docker volume prune' deletes them)"
        exit 0 ;;
    install) ;;
    *) echo "usage: $0 [install|images|stop|logs|remove]"; exit 1 ;;
esac

if ! command -v docker >/dev/null 2>&1; then
    say "installing Docker"
    apt-get update
    apt-get -y install curl ca-certificates
    curl -fsSL https://get.docker.com | sh
fi
systemctl enable --now docker >/dev/null 2>&1 || true

if ! docker compose version >/dev/null 2>&1 && ! command -v docker-compose >/dev/null 2>&1; then
    say "installing docker compose"
    apt-get -y install docker-compose-plugin || apt-get -y install docker-compose
fi

# The settings page asks for a code; make one if there is none yet.
if ! grep -q '^REGLAGES_CODE=.' .env 2>/dev/null; then
    code=$(od -An -N4 -tu4 /dev/urandom | tr -d ' ' | cut -c1-6)
    sed -i '/^REGLAGES_CODE=/d' .env 2>/dev/null || true
    echo "REGLAGES_CODE=$code" >> .env
fi
chmod 600 raspotify.conf hotspot.env .env 2>/dev/null || true

if [ ! -e /dev/snd ]; then
    warn "no sound card found (/dev/snd): the speaker will start but stay silent"
fi

if [ "$(envval AP_ENABLED hotspot.env)" = "yes" ]; then
    # The host's own copies would fight over the Wi-Fi card and DHCP.
    for svc in hostapd dnsmasq; do
        if systemctl is-active --quiet "$svc" 2>/dev/null; then
            warn "stopping the host's $svc (it would conflict)"
            systemctl stop "$svc" || true
            systemctl disable "$svc" >/dev/null 2>&1 || true
        fi
    done
fi

if [ -n "$BUILD" ]; then
    say "compiling the images and starting the containers (the first time takes a few minutes)"
else
    for name in spotify hotspot reglages; do
        docker image inspect "raspotify-docker/$name:latest" >/dev/null 2>&1 \
            || { warn "image raspotify-docker/$name is not loaded: run build-images.sh --pi first"; exit 1; }
    done
    say "starting the containers from the loaded images"
fi
# shellcheck disable=SC2086
compose up -d $BUILD --remove-orphans

port=$(envval REGLAGES_PORT .env); port=${port:-8080}
code=$(envval REGLAGES_CODE .env)
say "done. In the Spotify app, pick the speaker under 'Devices'."
say "settings page: http://$(hostname).local:$port  (code: ${code:-none})"
if [ "$(envval AP_ENABLED hotspot.env)" = "yes" ]; then
    say "Wi-Fi access point: $(envval AP_SSID hotspot.env)  (settings page there: http://$(envval AP_ADDRESS hotspot.env):$port)"
fi
compose ps
