# ToF on HAT J5

Observer owns one `microduck-tof` child, which reads the VL53L5CX at I²C3 address 0x29, 8×8 at 15 Hz. No tofd service is installed or enabled. The old socket subscriber is replaced with direct supervised capture. An explicit `MICRODUCK_TOF_BUS` may select another Linux I²C device; no bus scan runs.

The existing `i2c3-pihat` overlay is required. Install `deploy/99-microduck-tof-j5.rules` under `/etc/udev/rules.d/` and `deploy/tof-j5.conf` under `/etc/systemd/system/microduck-observer.service.d/`, preserving any existing files first. Reload udev rules and trigger only i2c-3, then daemon-reload and restart Observer using the normal idle deployment gate. Never give Observer unrestricted root access. J5 shares the bus with the audio codec; the ULD chunks transfers to 2 KiB and emits only distance/status.

`GET /api/v1/tof` returns availability, requested enabled state, actual streaming state, error, bus, address, rate and child PID. `POST` accepts exactly `{"enabled": true}` or false. `tof.json` beside calibration stores the choice, default off. Opening the web page does not switch the sensor on; closing the page does not switch it off. On disable the worker receives SIGTERM, stops ranging and closes its descriptors; after an 8-second deadline it is killed/reaped. Last-frame state is cleared. On capture failure descriptors are reaped before bounded 2–60 second retries; the UI reports offline instead of treating old distance as current. Invalid/oversized/non-monotonic frames are rejected. Negative ULD distances become invalid zones rather than unsigned wraparound.

Tests: `cargo test --locked --offline --bin microduck-observer --bin microduck-tof` in backend-rust; `npm test` and `npm run build` in frontend. Hardware verification must cover start, sustained 64-zone frames, stop/no worker, reopen, and unchanged servo torque. A successful stream is not a measurement-accuracy calibration.

The vendored ST ULD/firmware adds approximately 0.6 MB of source; capture has a separate small process and bounded 64-zone buffers. Source licenses and fixed upstream revision are in backend-rust/vendor/tof/UPSTREAM.md. No Python interpreter is used by the running sensor service.

## 2026-10-22 validation status

Deployed runtime-20261022-105705 on 192.168.137.23; rollback is roll-timer1p5s. Retains the latest 1.5-second roll contract, model files, calibration, gains, identity and camera configuration. J5-specific udev/systemd permissions installed. No servo motion or board reboot.

Native probe: 60 complete frames and clean SIGTERM exit 0. Production API: two on/off cycles, each with 40 distinct complete 64-zone snapshots, about 14.64 Hz; stop reaps the child and clears the latest frame. Enabled and disabled preferences both survive service restart. Final state is disabled. Observed distances were very close (API 0–14 mm; initial probe 4–37 mm); inspect obstruction/near field before treating them as accurate range.

Rust: 281 passed / 1 ignored; frontend: 86 passed; frontend and ARM builds passed. Model bank passed; isolated 20-second joints stream: 300/300 complete, 14.98 Hz; 20 production snapshots had 15 online untorqued servos. All seven frontend assets matched HTTP content hashes. Browser automation timed out, so visual click-through was not verified.

IMU was already unavailable before this release. User explicitly authorized proceeding with ToF deployment. A one-off staged verifier records imuVerified=false and retains joint/torque/config checks; the standard verifier is unchanged. This is not whole-robot sensor acceptance. The deployment report's legacy audio/inputPermissionsConfigured flags are fixed fields; this run used preserve-device-settings and did not reconfigure those permissions.

The first candidate accidentally used a binary overwritten by the concurrent roll build: ToF returned 404. Production was rolled back immediately, the current source was rebuilt, and the final binary (SHA256 903eec9fbfdf33b662817ad710a947de45b656a901678bfc7faadbfee2e523ea) passed actual API checks. Packaging now rejects binaries missing the ToF route marker.

Evidence: 实测记录/附件/平台/2026-10-22/tof-j5/.
