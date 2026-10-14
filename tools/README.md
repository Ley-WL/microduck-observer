# 当前工具

- `deploy-new-board.cmd` / `.ps1` / `.py`：纯净新主板一键部署，输入IP和sudo密码，安装验证过的Rust服务包，生成独立BLE/App身份，配置UART和自启并重启验证。已有身份/标定拒绝覆盖，`--check`只读。安装包版本和边界见 [NEW-BOARD.md](../deploy/NEW-BOARD.md)。

旧版本专用部署、已完成的试验/记录脚本以及附件中的旧Python源码副本已删除，不在此目录另建归档。实测原始数据、结果和当前供体Rust源码仍作为核查依据保留。

- `deploy-service-rust.py`：打包/安装当前Rust Wi-Fi和电源辅助；可指定 `--host`、`--port`、`--release`。默认生成唯一release名，不依赖旧staging。`--package-only`仅本机构包，不连接主板。
- `deploy-frontend.py`：只更新当前网页静态资源，沿用当前主板模型和二进制，原子切换独立release，不重启服务。`--package-only`仅本机构包；实际部署要求15舵机卸力，核对标定、增益、身份与主进程PID不变。

主Observer通用发布工具在 `deploy/`：`package_rust.py`、`verify_rust_hardware.py`、`switch_rust.py`。在线只读核查为 `check_live.py`、`verify-ducklink-rust.py` 和 `measure_runtime.py`。延迟/影子推理工具在 `scripts/`，测试在 `tests/` 和 `backend-rust/tests/`。

发布前仍须完成候选验证并确认主板没有动作、15舵机已卸力；本次清理不修改主板服务、模型或标定。

- `deploy-runtime.py`：在当前主板release基础上覆盖Rust主程序/网页，保留模型/标定/增益/BLE身份；先隔离只读候选验证，再原子切换并配置AIC3104音频权限，失败恢复旧版本。可指定host/port/package-only。
