#!/bin/bash
# Turns a Wi-Fi interface of the Pi into an access point.
set -eu

# Settings are re-read at every start, so the settings page only restarts us.
if [ -f /etc/raspotify/hotspot.env ]; then
    set -a
    # shellcheck disable=SC1091
    . /etc/raspotify/hotspot.env
    set +a
fi

if [ "${AP_ENABLED:-no}" != "yes" ]; then
    echo "[hotspot] access point off (AP_ENABLED=${AP_ENABLED:-no}); the Wi-Fi card is left alone"
    exec sleep infinity
fi

IFACE=${AP_INTERFACE:-wlan0}
SSID=${AP_SSID:-Raspotify}
PASS=${AP_PASSWORD:-}
CHANNEL=${AP_CHANNEL:-6}
COUNTRY=${AP_COUNTRY:-FR}
ADDR=${AP_ADDRESS:-192.168.50.1}
PREFIX=${AP_PREFIX:-24}
DHCP_START=${AP_DHCP_START:-192.168.50.10}
DHCP_END=${AP_DHCP_END:-192.168.50.100}
SHARE=${AP_SHARE_INTERNET:-yes}
UPLINK=${AP_UPLINK:-eth0}
HIDDEN=${AP_HIDDEN:-no}

log() { echo "[hotspot] $*"; }

if ! ip link show "$IFACE" >/dev/null 2>&1; then
    log "interface $IFACE not found; available:"; ip -o link | awk -F': ' '{print "  " $2}'
    exit 1
fi
if [ -n "$PASS" ] && [ "${#PASS}" -lt 8 ]; then
    log "the Wi-Fi password must have at least 8 characters"; exit 1
fi

rfkill unblock wlan 2>/dev/null || true

# Ask the Pi's NetworkManager (over the shared D-Bus) to leave the card to us,
# and give it back when we stop.
nm_managed() {
    [ -S /run/dbus/system_bus_socket ] || return 0
    local dev
    dev=$(dbus-send --system --print-reply --dest=org.freedesktop.NetworkManager \
        /org/freedesktop/NetworkManager org.freedesktop.NetworkManager.GetDeviceByIpIface \
        "string:$IFACE" 2>/dev/null | sed -n 's/.*object path "\(.*\)".*/\1/p')
    [ -n "$dev" ] || return 0
    dbus-send --system --print-reply --dest=org.freedesktop.NetworkManager "$dev" \
        org.freedesktop.DBus.Properties.Set string:org.freedesktop.NetworkManager.Device \
        string:Managed "variant:boolean:$1" >/dev/null 2>&1 \
        && log "NetworkManager managed $IFACE: $1"
}
nm_managed false
sleep 1

cleanup() {
    local code=${1:-0}
    log "stopping access point"
    pkill hostapd 2>/dev/null || true
    pkill dnsmasq 2>/dev/null || true
    if [ "$SHARE" = "yes" ]; then
        iptables -t nat -D POSTROUTING -s "${ADDR%.*}.0/${PREFIX}" -o "$UPLINK" -j MASQUERADE 2>/dev/null || true
        iptables -D FORWARD -i "$IFACE" -o "$UPLINK" -j ACCEPT 2>/dev/null || true
        iptables -D FORWARD -i "$UPLINK" -o "$IFACE" -m state --state RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || true
    fi
    ip addr flush dev "$IFACE" 2>/dev/null || true
    nm_managed true
    exit "$code"
}
trap 'cleanup 0' TERM INT

# --- Address of the Pi on its own network ---------------------------------
ip link set "$IFACE" down || true
ip addr flush dev "$IFACE"
ip addr add "${ADDR}/${PREFIX}" dev "$IFACE"
ip link set "$IFACE" up

# --- hostapd ---------------------------------------------------------------
hw_mode=g
[ "$CHANNEL" -gt 14 ] && hw_mode=a
ignore_ssid=0
[ "$HIDDEN" = "yes" ] && ignore_ssid=1
cat > /etc/hostapd/hostapd.conf <<EOF
interface=${IFACE}
driver=nl80211
ssid=${SSID}
country_code=${COUNTRY}
ieee80211d=1
hw_mode=${hw_mode}
channel=${CHANNEL}
ieee80211n=1
wmm_enabled=1
auth_algs=1
ignore_broadcast_ssid=${ignore_ssid}
EOF
if [ -n "$PASS" ]; then
cat >> /etc/hostapd/hostapd.conf <<EOF
wpa=2
wpa_key_mgmt=WPA-PSK
rsn_pairwise=CCMP
wpa_passphrase=${PASS}
EOF
else
    log "WARNING: open network (no password)"
fi

# --- dnsmasq: DHCP for the phones -------------------------------------------
cat > /etc/dnsmasq.d/hotspot.conf <<EOF
interface=${IFACE}
bind-dynamic
except-interface=lo
dhcp-range=${DHCP_START},${DHCP_END},255.255.255.0,12h
dhcp-option=option:router,${ADDR}
dhcp-option=option:dns-server,${ADDR}
domain-needed
bogus-priv
EOF

# --- Share the Pi's internet (Spotify needs it) ------------------------------
if [ "$SHARE" = "yes" ]; then
    echo 1 > /proc/sys/net/ipv4/ip_forward || log "could not enable forwarding"
    iptables -t nat -C POSTROUTING -s "${ADDR%.*}.0/${PREFIX}" -o "$UPLINK" -j MASQUERADE 2>/dev/null \
        || iptables -t nat -A POSTROUTING -s "${ADDR%.*}.0/${PREFIX}" -o "$UPLINK" -j MASQUERADE
    iptables -C FORWARD -i "$IFACE" -o "$UPLINK" -j ACCEPT 2>/dev/null \
        || iptables -A FORWARD -i "$IFACE" -o "$UPLINK" -j ACCEPT
    iptables -C FORWARD -i "$UPLINK" -o "$IFACE" -m state --state RELATED,ESTABLISHED -j ACCEPT 2>/dev/null \
        || iptables -A FORWARD -i "$UPLINK" -o "$IFACE" -m state --state RELATED,ESTABLISHED -j ACCEPT
    log "sharing internet from $UPLINK"
fi

dnsmasq --conf-dir=/etc/dnsmasq.d --keep-in-foreground --log-facility=- &
log "network \"$SSID\" on $IFACE (channel $CHANNEL, $COUNTRY), Pi at $ADDR"
hostapd /etc/hostapd/hostapd.conf &
wait -n
log "hostapd or dnsmasq stopped"
cleanup 1
