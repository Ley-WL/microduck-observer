#!/bin/sh
# Build a staging tree only; never restarts the robot or changes current.
# Args: ZOO_ROOT RKNN_SDK MPP_SOURCE MPP_LIB_DIR MODEL_DIR OUTPUT_DIR
set -eu
[ "$#" = 6 ] || { echo 'Expected ZOO SDK MPP MPP_LIBS MODELS OUT' >&2; exit 2; }
Z=$1; SDK=$2; MPP=$3; LIBS=$4; MODELS=$5; OUT=$6
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
mkdir -p "$OUT/bin" "$OUT/lib" "$OUT/models/vision"
[ "$(sha256sum "$MODELS/yolov5n.rknn" | cut -d' ' -f1)" = decacfb0d94c0a0698a8dbcc64df7d3e7737d000d80886ff5b8cc51cb6f03295 ]
[ "$(sha256sum "$MODELS/RetinaFace_mobile320.rknn" | cut -d' ' -f1)" = ed18d5a0902a17cb592ac5cf2ced014a3f1e011d9fff5d69c627d1fe975aa463 ]
sh "$ROOT/tools/build-vision-worker.sh" "$Z" "$SDK" "$OUT/build"
cp "$OUT/build/microduck-vision-worker" "$OUT/bin/"
sh "$ROOT/tools/build-camera-mpp.sh" "$MPP" "$Z/3rdparty/librga/include" "$LIBS" "$OUT/bin/microduck-camera-mpp"
cp "$SDK/librknnrt.so" "$OUT/lib/"
cp "$LIBS/librga.so" "$LIBS/librockchip_mpp.so.1" "$OUT/lib/"
cp "$MODELS/yolov5n.rknn" "$MODELS/RetinaFace_mobile320.rknn" "$OUT/models/vision/"
cp "$ROOT/deploy/vision.conf" "$OUT/vision.conf"
(cd "$OUT" && sha256sum bin/* lib/* models/vision/* vision.conf > SHA256SUMS)
