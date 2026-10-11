# 原生 Rust 部署

## 2026-10-11 板上服务迁移

Wi-Fi 管理与受限电源辅助源码分别为 `backend-rust/src/bin/ducklink-wifi.rs` 和 `backend-rust/src/bin/microduck-power-helper.rs`。构建使用 `cargo build --release --locked --target aarch64-unknown-linux-gnu --bins`；打包入口包含两个 ARM64 helper。`tools/deploy-service-rust.py` 安装独立 `/usr/local/lib/microduck-services/20261011-service-rust-v1/`，替换 systemd ExecStart，保持原 Unix socket 协议和权限隔离，主控制服务不升权、不重启。

Wi-Fi 保留 wpa_supplicant 控制 socket、异步连接、成功关联及 IPv4 后保存 netplan、连接失败恢复原选择、原配置原子写/生成失败回滚。密码不进入命令行、日志或错误回包。主板系统提供的 netplan 命令属于系统网络工具，机器人服务本身不再以 Python 运行。电源 helper 仍只允许 poweroff/reboot，经 systemd 延迟五秒执行；自动验收仅发非法动作，不真正关机/重启。

部署要求空闲且15舵机卸力；检查当前关联、netplan字节、模型/二进制/标定/持久增益/BLE身份不变及20快照。旧单位和 Python helper 归档在 `/var/lib/microduck-observer/service-rust-backup-20261011/` 用于失败回滚，活动位置的 Python helper 删除。构建、部署、诊断、实测附件中的一次性 Python 脚本保留；历史专用部署入口不能视为当前平台整套安装入口。

> **2026-10-11 最新执行覆盖：** 已部署kissqy-mechanical-v1，全部15关节机械范围采用kissqy保存标定。模型先裁机械角度、再按本机零位/方向转4096编码，无跨圈/编码饱和；单圈编码或EEPROM有效边界不满足则拒绝。模型权重、raw历史、增益及其它设置保持。旧训练XML/v3限位合同属于训练历史，和当前机械发令 guard 分开核验；旧段落的“仅v3裁角度/原供体编码饱和”已被本条覆盖。103检查、ARM五技能及152帧目标公式核对通过，只读部署非起身实机成功，详见实测03/04及2026-10-11/kissqy-mechanical证据。

## 主板电源按钮

页面右上角提供关机/重启，确认后POST `/api/v1/system/poweroff` 或 `/api/v1/system/reboot`，请求必须包含当前info的bootId和confirm=true。模拟器禁用；实机须配置`MICRODUCK_POWER_CONTROL=1`。接口沿用JSON和Origin校验，超时/调度失败不显示成功，也不自动重试。

`microduck-power.socket`创建仅radxa可访问的0600 Unix socket；实例service以root运行固定的 Rust `microduck-power-helper`，只接受两种动作，经systemd-run延迟5秒调用systemctl。不放宽观测服务权限、不配置任意sudo命令。启用socket后用power.conf添加环境变量。部署脚本保留旧release及标定备份，启动失败回滚。

关机前托稳机器人；主板关机不切断舵机电源。等待约30秒并确认关机完成后再断电；页面断线本身不是关机完成证明。关机按钮需要重新供电才能再次启动主板。重启按钮提交后页面自动重连。接口和权限验证可用非法动作/缺确认请求，禁止将实际关机当作无影响的自动测试。

后端源码、环境变量与测试见[backend-rust](../backend-rust/README.md)。前端不变。仓库旧Python后端已删除；主板历史release保留为回滚来源，旧版web-debug不修改。

生产目标为Linux ARM64 GNU。可在编译机安装`aarch64-unknown-linux-gnu`目标，用交叉编译器编译：

```bash
rustup target add aarch64-unknown-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
cargo build --release --locked --target aarch64-unknown-linux-gnu -j 2
```

## 打包

```bash
python deploy/package_rust.py \
  --binary backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer
```

发布包包含ARM64二进制、原前端dist、现有v5模型和native service；不包含标定、凭据、虚拟环境或Python源文件。`--frontend`可指定从当前release复用的前端资源。ONNX Runtime为独立动态库，须按其许可证安装合适的ARM64版本，在新release的`lib/libonnxruntime.so`链接至已安装库。原Python wheel中的原生库也可复用，Rust直接加载C API，不启动Python；保留原位置用于回滚。

## 切换与回滚

1. 解包到新的`/home/radxa/microduck-observer/releases/<版本>`，不要覆盖旧release。保存current路径、systemd主service和drop-in，以及标定文件的备份与SHA256。
2. 使用复制的标定文件先启动模拟候选服务，核对HTTP/WS、静态页面与模型路径。候选进程不得与生产服务同时打开硬件串口。
3. 实机只读验证前确认当前没有运动任务；暂时停止旧服务，在隔离端口8878运行候选服务。读取IMU与15颗舵机，记录实际扫描频率、网页接收频率、CPU与缺包率。测试完成先退出候选进程，再恢复旧服务或切换。
4. 通过后将current链接至新release，使用`microduck-observer-rust.service`作为`/etc/systemd/system/microduck-observer.service`，保留现有串口drop-in；执行`daemon-reload`和`restart`。共享标定继续指向`/var/lib/microduck-observer/calibration.json`，不从发布包替换。
5. 检查systemd active、进程为native binary、HTTP/WS有效、前端文件SHA256与原release一致、标定references/directions/安装矩阵未改变。普通采集与服务启动不写舵机目标或扭矩。

回滚：停止native服务，恢复此前current链接和备份的主service，保留原drop-in，`daemon-reload`并重启；读取共享标定，不覆盖新标定。必要时管理员从`.previous.json`或部署备份恢复，但不能把备份当作始终可覆盖的当前数据。

生产切换只代表服务部署；软件轨迹测试、只读采集及ONNX数值一致不能代替实际站立、运动或硬件中位验收。实测记录以[实测索引](../docs/实测记录/README.md)的最新日期为准。
