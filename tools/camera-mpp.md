# Native camera runtime

`camera-mpp.cc` consumes tightly packed 640x360 NV12 frames at 30 fps from
stdin, keeps one frame in three, and writes concatenated hardware JPEG frames
to stdout. Diagnostics go to stderr. Observer owns and reaps this helper and
the V4L2 capture child under the same systemd service. There is no software
codec fallback and no standalone server or Python runtime.

Pipeline: cached DMA buffers -> RGA NV12/RGB conversion -> fixed RGB lookup
tables (gains 1.48/1/1.80, gamma 1.15) -> RGA RGB/NV12 -> MPP JPEG quality 80.
DMA handles and frame buffers are reused. CPU access is bracketed by DMA
cache synchronization. The web UI retains clockwise 90-degree display rotation.
The fixed color correction is not automatic exposure or white balance.

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
