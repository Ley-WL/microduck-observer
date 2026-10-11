# 当前工具

旧版本专用部署、已完成的试验/记录脚本以及附件中的旧Python源码副本已删除，不在此目录另建归档。实测原始数据、结果和当前供体Rust源码仍作为核查依据保留。

- `deploy-service-rust.py`：打包/安装当前Rust Wi-Fi和电源辅助；可指定 `--host`、`--port`、`--release`。默认生成唯一release名，不依赖旧staging。`--package-only`仅本机构包，不连接主板。

主Observer通用发布工具在 `deploy/`：`package_rust.py`、`verify_rust_hardware.py`、`switch_rust.py`。在线只读核查为 `check_live.py`、`verify-ducklink-rust.py` 和 `measure_runtime.py`。延迟/影子推理工具在 `scripts/`，测试在 `tests/` 和 `backend-rust/tests/`。

发布前仍须完成候选验证并确认主板没有动作、15舵机已卸力；本次清理不修改主板服务、模型或标定。
