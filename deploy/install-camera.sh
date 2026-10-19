#!/bin/sh
# Run as root after staging a camera-capable Rust Observer binary and frontend.
# This prepares permissions/state and retires the legacy Python camera service.
# It does not restart Observer or reboot the board. Schedule the switch safely.
set -eu
[ "$(id -u)" = 0 ] || { echo 'Run as root' >&2; exit 1; }
source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
[ "$(tr -d '\000' </proc/device-tree/model)" = 'Radxa ZERO 3' ]
for command in ffmpeg media-ctl v4l2-ctl; do command -v "$command" >/dev/null; done
overlay=radxa-zero3-rpi-camera-v2
[ -f "/boot/dtb/rockchip/overlay/$overlay.dtbo" ]
[ "$(grep -c '^overlays=' /boot/armbianEnv.txt)" = 1 ]
backup="/var/lib/microduck-observer/camera-migration-$(date +%Y%m%d-%H%M%S-%N)"
mkdir -p "$backup"
cp -p /boot/armbianEnv.txt "$backup/armbianEnv.txt"
if ! grep '^overlays=' /boot/armbianEnv.txt | cut -d= -f2- | tr ' ' '\n' | grep -qx "$overlay"; then
    sed -i "/^overlays=/s/\$/ $overlay/" /boot/armbianEnv.txt
    echo 'Camera overlay added; a safe board reboot is required.'
fi
settings=/var/lib/microduck-observer/camera.json
if [ ! -e "$settings" ]; then
    if [ -f /var/lib/microduck-camera/settings.json ]; then
        install -o radxa -g radxa -m 600 /var/lib/microduck-camera/settings.json "$settings"
    else
        printf '{"enabled":false}\n' >"$settings"
        chown radxa:radxa "$settings"
        chmod 600 "$settings"
    fi
fi
cp -p "$settings" "$backup/camera.json"
if systemctl cat microduck-camera.service >/dev/null 2>&1; then
    systemctl is-enabled microduck-camera.service >"$backup/legacy-enabled.txt" || true
    systemctl disable --now microduck-camera.service
fi
if [ -f /etc/systemd/system/microduck-camera.service ]; then
    mv /etc/systemd/system/microduck-camera.service "$backup/"
fi
if [ -d /opt/microduck-camera ]; then
    mv /opt/microduck-camera "$backup/legacy-camera"
fi
mkdir -p /etc/systemd/system/microduck-observer.service.d
if [ -f /etc/systemd/system/microduck-observer.service.d/camera.conf ]; then
    cp -p /etc/systemd/system/microduck-observer.service.d/camera.conf "$backup/previous-camera.conf"
fi
install -m 644 "$source_dir/camera.conf" /etc/systemd/system/microduck-observer.service.d/camera.conf
systemctl daemon-reload
echo "Camera permissions and saved preference prepared; legacy backup: $backup"
