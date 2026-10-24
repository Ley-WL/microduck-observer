#!/bin/sh
# Build only. Does not install, load, bind or modify boot configuration.
set -eu
out=${1:?Usage: build.sh ABSOLUTE_NEW_BUILD_DIRECTORY}
case "$out" in /*) ;; *) echo 'Build directory must be absolute' >&2; exit 2;; esac
mkdir "$out"
curl -fL 'https://raw.githubusercontent.com/armbian/linux-rockchip/372ce582b94a3360a29727c3c49c88bb7509da22/drivers/media/i2c/imx219.c' -o "$out/imx219.c"
printf '%s  %s\n' c98f6e38b1efa41cd8eccb05815a838a2d7ceca945ff23084115ca306a93be63 "$out/imx219.c" | sha256sum -c -
patch -d "$out" -p1 < "$(dirname "$0")/native-vga.patch"
mv "$out/imx219.c" "$out/imx219_microduck.c"
printf 'obj-m += imx219_microduck.o\n' > "$out/Makefile"
make -C "/lib/modules/$(uname -r)/build" M="$out" modules
