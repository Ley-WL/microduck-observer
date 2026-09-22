#!/bin/sh
set -eu
# Invoked with sudo after uploading a built release and preparing its venv.
release="$1"
case "$release" in /home/radxa/microduck-observer/releases/*) ;; *) echo 'Invalid release path' >&2; exit 1;; esac
test -f "$release/debug-server/server.py"
test -f "$release/frontend/dist/index.html"
test -x /home/radxa/microduck-observer/venv/bin/python
if systemctl is-active --quiet microduck-observer; then
  systemctl stop microduck-observer
fi
if fuser /dev/i2c-4 >/dev/null 2>&1; then
  echo 'I2C is owned by another process; refusing to start observer' >&2
  exit 1
fi
getent group i2c >/dev/null || groupadd --system i2c
printf '%s\n' 'SUBSYSTEM=="i2c-dev", KERNEL=="i2c-4", GROUP="i2c", MODE="0660"' > /etc/udev/rules.d/70-microduck-observer-i2c.rules
udevadm control --reload-rules
chgrp i2c /dev/i2c-4
chmod 0660 /dev/i2c-4
base=/home/radxa/microduck-observer
if test -L "$base/current"; then
  readlink "$base/current" > "$base/previous-release.txt"
fi
ln -sfn "$release" "$base/current.next"
mv -Tf "$base/current.next" "$base/current"
install -m 0644 "$release/deploy/microduck-observer.service" /etc/systemd/system/microduck-observer.service
systemctl daemon-reload
systemctl enable --now microduck-observer
systemctl --no-pager status microduck-observer
