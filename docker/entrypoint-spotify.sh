#!/bin/bash
# Starts the Spotify Connect receiver inside the container.
# Same defaults as raspotify's systemd unit, same restart rule as its
# crash-report generator (restart after 10 s, give up after 6 failures in 2 min).
set -u

CONF=/etc/raspotify/conf
CRASH_REPORT=/var/lib/raspotify/crash_report
RESTART_DELAY=${RESTART_DELAY:-10}
FAIL_WINDOW=${FAIL_WINDOW:-120}
FAIL_LIMIT=${FAIL_LIMIT:-6}
AVAHI_MODE=${AVAHI_MODE:-container}

log() { echo "[spotify] $*"; }

# --- Defaults taken from raspotify.service --------------------------------
host_name="${HOST_HOSTNAME:-$(hostname)}"
export LIBRESPOT_NAME="raspotify (${host_name})"
export LIBRESPOT_BACKEND="alsa"
export LIBRESPOT_CACHE="/var/cache/raspotify"
export LIBRESPOT_SYSTEM_CACHE="/var/lib/raspotify"
mkdir -p /run/raspotify /var/cache/raspotify /var/lib/raspotify
chmod 700 /var/cache/raspotify /var/lib/raspotify
export TMPDIR="/run/raspotify"

# --- The configuration file overrides the defaults (EnvironmentFile=) -------
if [ -f "$CONF" ]; then
    set -a
    # shellcheck disable=SC1090
    . "$CONF"
    set +a
    log "configuration loaded from $CONF"
else
    log "no $CONF found, using defaults"
fi

# --- Zeroconf announcement through Avahi ------------------------------------
if [ "$AVAHI_MODE" = "host" ]; then
    if [ -S /run/dbus/system_bus_socket ]; then
        log "using the host's avahi-daemon"
    else
        log "AVAHI_MODE=host but /run/dbus is not mounted: the speaker will not be visible"
    fi
else
    rm -f /run/dbus/pid /run/avahi-daemon/pid
    mkdir -p /run/dbus
    dbus-daemon --system --fork
    avahi-daemon --daemonize --no-chroot --no-drop-root || log "avahi-daemon did not start"
fi

write_crash_report() {
    {
        echo "-- System Info --"; echo
        uname -a
        cat /etc/os-release 2>/dev/null
        echo; echo "-- Config --"; echo
        if [ -f "$CONF" ]; then
            while read -r line; do
                line="$(echo "$line" | awk '{$1=$1};1')"
                case "$line" in
                    LIBRESPOT_USERNAME*) echo "LIBRESPOT_USERNAME=XXXXXXXX" ;;
                    LIBRESPOT_PASSWORD*) echo "LIBRESPOT_PASSWORD=XXXXXXXX" ;;
                    LIBRESPOT_ACCESS_TOKEN*) echo "LIBRESPOT_ACCESS_TOKEN=XXXXXXXX" ;;
                    LIBRESPOT_PROXY*) echo "LIBRESPOT_PROXY=XXXXXXXX" ;;
                    LIBRESPOT_*) echo "$line" ;;
                esac
            done < "$CONF"
        fi
        echo; echo "-- Output of aplay -l --"; echo
        aplay -l
        echo; echo "-- Output of aplay -L --"; echo
        aplay -L
        echo; echo "-- Output of librespot -d ? --"
        librespot -d "?"
    } > "$CRASH_REPORT" 2>/dev/null
    chmod 600 "$CRASH_REPORT"
}

stop_requested=0
child=0
on_term() { stop_requested=1; [ "$child" -ne 0 ] && kill -TERM "$child" 2>/dev/null; }
trap on_term TERM INT

failures=()
while :; do
    log "starting librespot as \"$LIBRESPOT_NAME\""
    librespot &
    child=$!
    wait "$child"
    code=$?
    child=0
    [ "$stop_requested" -eq 1 ] && exit 0
    [ "$code" -eq 0 ] && { log "librespot exited normally"; exit 0; }

    now=$(date +%s)
    kept=()
    for t in "${failures[@]}"; do
        [ $((now - t)) -lt "$FAIL_WINDOW" ] && kept+=("$t")
    done
    kept+=("$now")
    failures=("${kept[@]}")
    log "librespot failed (code $code), ${#failures[@]} failure(s) in the last ${FAIL_WINDOW}s"
    write_crash_report

    if [ "${#failures[@]}" -ge "$FAIL_LIMIT" ]; then
        log "too many failures, giving up (see $CRASH_REPORT)"
        exit 1
    fi
    sleep "$RESTART_DELAY"
done
