> 2026-10-20 current: full 3280×2464 sensor/ISP crop → hardware-scaled 640×480, no crop. Nominal 21 fps capture, one in two frames → 10.5 fps preview; 480×640 upright, v2 RGB IPC (921600 bytes). Deployed on 192.168.137.238; earlier 360/480 notes below are historical.

# Native camera runtime

`camera-mpp.cc` consumes tightly packed 640x480 NV12 frames at nominal 21 fps from
stdin, keeps one frame in two (nominal 10.5 fps), and writes concatenated hardware JPEG frames
to stdout. Diagnostics go to stderr. Observer owns and reaps this helper and
the V4L2 capture child under the same systemd service. There is no software
codec fallback and no standalone server or Python runtime.

Sensor: full 3280x2464 RAW10 -> ISP full-frame hardware scaling to 640x480 NV12.
The full sensor and ISP crop rectangles are retained; the ~0.16% aspect mismatch
is accepted to preserve the entire field of view. CPU buffers never contain the
full-resolution RAW image.

Pipeline: cached DMA buffers -> RGA full-range BT.601 NV12/RGB conversion -> fixed RGB lookup
tables (gains 1.46/1/1.57, gamma 1.30) -> RGA full-range BT.601 RGB/NV12 -> MPP JPEG quality 80.
DMA handles and frame buffers are reused. CPU access is bracketed by DMA
cache synchronization. The web UI retains clockwise 90-degree display rotation.
The fixed color correction is not automatic exposure or white balance.
The 2026-10-20 neutral-paper center calibration reduced the red deficit from
5.21% to 0.74% in that scene; edge color shading remains. This is not a
full-frame lens-shading or color-chart calibration.

Dependencies tested on Radxa ZERO 3 / RK3566, vendor kernel 6.1.115:

- MPP source: `rockchip-linux/mpp`, commit
  `20959ec3dabb313b8c33ca0b7b09c06ffd2a9caa`, Release ARM64 build.
- RGA headers/library: `airockchip/rknn_model_zoo`, commit
  `bad6c7334531becaf90a561988519b7bec34d0ab`, Linux/aarch64 library.
- RGA kernel driver 1.3.7, MPP/VEPU driver and DMA heap devices.

Build with the ARM64 cross compiler:

```sh
sh tools/build-camera-mpp.sh MPP_SOURCE RGA_INCLUDE LIB_DIR microduck-camera-mpp
```

`LIB_DIR` must contain `librockchip_mpp.so.1` and `librga.so`. Keep third-party
sources, binaries and their license notices in the local dependency/release
bundle, not in the source repository. Install this release layout:

```text
bin/microduck-observer
bin/microduck-camera-mpp
lib/librockchip_mpp.so.1
lib/librga.so
```

The helper's RUNPATH is `$ORIGIN/../lib`. Do not overwrite system libraries.
Runtime-only updates that clone the active release retain these files; fresh
packages must explicitly include them. Camera-enabled releases without the
helper fail clearly instead of silently reverting to FFmpeg.

Install `deploy/camera.conf` and `deploy/70-microduck-camera-mpp.rules` together;
the video group gets access only to the required RGA, MPP and system DMA heaps.
Keep `KillMode=control-group` and the service's device allowlist. Existing
`camera.json` is retained and new installations default to disabled. Follow
the hardware deployment checks before restarting Observer; keep the previous
release and drop-in/udev backups for rollback.

Validation performed: empty input exits 0, truncated frame exits 1, arguments
exit 2; 100 input frames produce 34 decodable 640x360 JPEGs; camera unit tests;
live enable/disable cleanup; saved off/on across service restarts; hardware
device access under the unprivileged Observer service; live stream and CPU
sampling. The board reboot itself was not repeated for this replacement.
Evidence is in `docs/实测记录/05-摄像头实测.md` (2026-10-19).

2026-10-20 deployed runtime-20261020-camera-480: 640x480 capture, 480x640 upright view, aligned MPP vertical stride 480. RGB IPC uses the new v2 magic and 921600-byte frame. Observer, camera helper and vision helper must be deployed together; preceding 640x360 validation remains historical.

Live MPP JPEG decoded as 640x480 and preview measured about 9.9fps; 261 Rust tests and native/IPC checks passed. See 2026-10-20 vision-480 evidence for detector load and dropped/duplicate requests.

2026-10-20 latest: restored v4b with 640x360 capture and 360x640 upright view. The 640x480 experiment stretched the active 1920x1080 sensor/ISP input; sensor mode was not changed. Current runtime uses v1 RGB IPC (691200 bytes) and 368-row aligned MPP buffers.

Full-resolution exposure startup: the vendor sensor mode table writes exposure=500 during STREAMON. Observer primes the cached control to 1499 before starting capture, then writes the intended 1500 after the first complete JPEG. Using a different priming value prevents V4L2 from suppressing the post-start write as unchanged. A failed write shuts down the owned pipeline with a visible error. This is fixed exposure, not auto exposure; gain and LUT are unchanged.
