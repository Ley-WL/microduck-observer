#!/bin/sh
# Pinned Zoo v2.3.2 + existing RKNN SDK (rknn_api.h, librknnrt.so, librga.so).
set -eu
[ "$#" = 3 ] || { echo 'Expected ZOO_ROOT SDK_DIR OUTPUT_DIR' >&2; exit 2; }
Z=$1; SDK=$2; OUT=$3
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
CC=${CC:-aarch64-linux-gnu-gcc}; CXX=${CXX:-aarch64-linux-gnu-g++}
mkdir -p "$OUT"
"$CC" -O2 -I"$Z/utils" -c "$Z/utils/file_utils.c" -o "$OUT/file_utils.o"
for m in yolov5 RetinaFace; do
    EXTRA=
    [ "$m" != yolov5 ] || EXTRA=-DVISION_PERSON
    "$CXX" -O2 -std=c++17 $EXTRA -I"$Z/utils" -I"$SDK" -I"$Z/examples/$m/cpp" \
        -c "$ROOT/tools/vision-model.cc" -o "$OUT/$m-adapter.o"
    if [ "$m" = yolov5 ]; then SRC=yolov5; else SRC=retinaface; fi
    "$CXX" -O2 -std=c++17 -I"$Z/utils" -I"$SDK" -I"$Z/examples/$m/cpp" \
        -c "$Z/examples/$m/cpp/rknpu2/$SRC.cc" -o "$OUT/$m.o"
done
"$CXX" -O2 -std=c++17 -I"$Z/utils" -I"$SDK" -I"$Z/examples/yolov5/cpp" \
    -c "$Z/examples/yolov5/cpp/postprocess.cc" -o "$OUT/postprocess.o"
"$CXX" -O2 -std=c++17 -Wall -Wextra -I"$Z/utils" -I"$SDK" -I"$Z/3rdparty/librga/include" \
    "$ROOT/tools/vision-worker.cc" "$OUT/"*.o -L"$SDK" -lrknnrt -lrga -lm \
    '-Wl,-rpath,$ORIGIN/../lib' -o "$OUT/microduck-vision-worker"
