#!/bin/sh
# Usage: build-camera-mpp.sh MPP_SOURCE RGA_HEADERS LIB_DIR OUTPUT
# LIB_DIR contains librockchip_mpp.so.1 and librga.so (aarch64).
set -eu
[ "$#" = 4 ] || { echo 'Expected MPP_SOURCE RGA_HEADERS LIB_DIR OUTPUT' >&2; exit 2; }
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
"${CXX:-aarch64-linux-gnu-g++}" -O3 -std=c++17 -Wall -Wextra \
    "$ROOT/tools/camera-mpp.cc" -I"$1/inc" -I"$2" -L"$3" \
    -l:librockchip_mpp.so.1 -lrga -lm '-Wl,-rpath,$ORIGIN/../lib' -o "$4"
