#!/bin/sh
# Compile the three Raspotify Docker images.
#
#   ./build-images.sh                      on the Pi: compile for this Pi (= docker compose build)
#   ./build-images.sh --pi                 on a computer: compile for a 64-bit Pi and save
#                                          raspotify-images-arm64.tar.gz (copy it to the Pi,
#                                          then: gunzip -c raspotify-images-arm64.tar.gz | docker load)
#   ./build-images.sh --pi --envoyer pi@raspberrypi.local
#                                          same, then load the images straight onto the Pi (SSH)
#   --32bits                               for a Pi running a 32-bit system (armhf)
#
# Then, on the Pi:  docker compose -f docker-compose.images.yml up -d   (nothing is compiled there)
set -e
cd "$(dirname "$0")"

say() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m==>\033[0m %s\n' "$*" >&2; exit 1; }

cross=no
platform=linux/arm64
arch=arm64
target=""
while [ $# -gt 0 ]; do
    case "$1" in
        --pi) cross=yes ;;
        --32bits) platform=linux/arm/v7; arch=armhf ;;
        --envoyer) shift; target="$1"; [ -n "$target" ] || fail "--envoyer needs user@pi" ;;
        -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
        *) fail "unknown option: $1 (see --help)" ;;
    esac
    shift
done

command -v docker >/dev/null 2>&1 || fail "Docker is not installed on this machine"
deb_url=$(sed -n 's/^RASPOTIFY_DEB_URL=//p' .env 2>/dev/null | head -n1)
deb_url=${deb_url:-auto}
images="spotify hotspot reglages"

if [ "$cross" = no ]; then
    say "compiling the images for this machine ($(uname -m))"
    if docker compose version >/dev/null 2>&1; then
        docker compose build
    else
        docker-compose build
    fi
    say "done: docker compose up -d   (or sudo ./install.sh)"
    exit 0
fi

# --- Compile on a computer for the Pi's processor -----------------------------
docker buildx version >/dev/null 2>&1 || fail "docker buildx is missing (package docker-buildx-plugin)"
if ! docker buildx ls 2>/dev/null | grep -q "$platform"; then
    say "this computer cannot run $platform programs yet; Docker needs QEMU emulation for that:"
    echo "    docker run --privileged --rm tonistiigi/binfmt --install all"
    printf 'Run it now? [y/N] '
    read -r answer
    case "$answer" in
        y|Y|o|O|oui|yes) docker run --privileged --rm tonistiigi/binfmt --install all ;;
        *) fail "stopped: run the command above, then this script again" ;;
    esac
fi

for name in $images; do
    say "compiling raspotify-docker/$name for $platform (emulated: this can take several minutes)"
    docker buildx build --platform "$platform" \
        --build-arg RASPOTIFY_DEB_URL="$deb_url" \
        -f "Dockerfile.$name" -t "raspotify-docker/$name:latest" --load .
done

list=""
for name in $images; do list="$list raspotify-docker/$name:latest"; done

if [ -n "$target" ]; then
    say "sending the images to $target"
    # shellcheck disable=SC2086
    docker save $list | gzip | ssh "$target" 'gunzip | (docker load 2>/dev/null || sudo -n docker load)'
    say "done. On the Pi, in the raspotify-docker folder: docker compose -f docker-compose.images.yml up -d"
else
    out="raspotify-images-$arch.tar.gz"
    say "saving the images to $out"
    # shellcheck disable=SC2086
    docker save $list | gzip > "$out"
    say "done: copy $out and this folder to the Pi, then on the Pi:"
    echo "    gunzip -c $out | docker load"
    echo "    docker compose -f docker-compose.images.yml up -d"
fi
