#!/bin/sh
# Usage: sh tools/build-rknn-detect-bench.sh ZOO_ROOT RKNN_SDK_DIR OUTPUT_DIR
# ZOO_ROOT: Rockchip rknn_model_zoo v2.3.2, including utils and 3rdparty headers.
# RKNN_SDK_DIR: rknn_api.h, aarch64 librknnrt.so and STB image headers.
# USE_RGA=1 additionally requires matching aarch64 librga.so in SDK_DIR.
set -eu
if [ "$#" -ne 3 ]; then echo 'Expected ZOO_ROOT RKNN_SDK_DIR OUTPUT_DIR' >&2; exit 2; fi
Z=$1
SDK=$2
OUT=$3
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
CC=${CC:-aarch64-linux-gnu-gcc}
CXX=${CXX:-aarch64-linux-gnu-g++}
mkdir -p "$OUT/build"
RGA_FLAGS=-DDISABLE_RGA
RGA_LINK=
if [ "${USE_RGA:-0}" = 1 ]; then
    RGA_FLAGS=
    RGA_LINK=-lrga
fi
for f in image_utils file_utils image_drawing; do
    SOURCE="$Z/utils/$f.c"
    if [ "${USE_RGA:-0}" = 1 ] && [ "$f" = image_utils ]; then
        # RGB888 needs a four-pixel stride alignment on this RGA2 path.
        # Preserve the upstream 16-pixel check for other pixel formats.
        grep -q 'src_img->width % 16 == 0 && dst_img->width % 16 == 0' "$SOURCE" || {
            echo 'Unsupported image_utils version; expected Zoo v2.3.2' >&2; exit 1;
        }
        sed 's/src_img->width % 16 == 0 \&\& dst_img->width % 16 == 0/(src_img->width % 16 == 0 \&\& dst_img->width % 16 == 0) || (src_img->format == IMAGE_FORMAT_RGB888 \&\& dst_img->format == IMAGE_FORMAT_RGB888 \&\& src_img->width % 4 == 0 \&\& dst_img->width % 4 == 0)/' \
            "$SOURCE" > "$OUT/build/image_utils.c"
        SOURCE="$OUT/build/image_utils.c"
    fi
    "$CC" -O2 -include math.h $RGA_FLAGS -DDISABLE_LIBJPEG \
        -I"$Z/utils" -I"$SDK" -I"$Z/3rdparty/stb" -I"$Z/3rdparty/librga/include" \
        -c "$SOURCE" -o "$OUT/build/$f.o"
done
for m in yolov5 RetinaFace; do
    if [ "$m" = yolov5 ]; then
        set -- -DBENCH_YOLO "$Z/examples/$m/cpp/rknpu2/yolov5.cc" "$Z/examples/$m/cpp/postprocess.cc"
    else
        set -- "$Z/examples/$m/cpp/rknpu2/retinaface.cc"
    fi
    "$CXX" -O2 -std=c++17 -I"$Z/utils" -I"$SDK" -I"$Z/examples/$m/cpp" \
        "$ROOT/tools/rknn-detect-bench.cc" "$@" \
        "$OUT/build/image_utils.o" "$OUT/build/file_utils.o" "$OUT/build/image_drawing.o" \
        -L"$SDK" -lrknnrt $RGA_LINK -Wl,--wrap=rknn_run -Wl,--wrap=rknn_init \
        -Wl,--wrap=convert_image_with_letterbox -Wl,--wrap=rknn_inputs_set \
        -Wl,--wrap=rknn_outputs_get -lm -o "$OUT/$m-bench"
done
