# 原生 Rust 部署

## 主板电源按钮

页面右上角提供关机/重启，确认后POST `/api/v1/system/poweroff` 或 `/api/v1/system/reboot`，请求必须包含当前info的bootId和confirm=true。模拟器禁用；实机须配置`MICRODUCK_POWER_CONTROL=1`。接口沿用JSON和Origin校验，超时/调度失败不显示成功，也不自动重试。

`microduck-power.socket`创建仅radxa可访问的0600 Unix socket；实例service以root运行固定的`power-helper.py`，只接受两种动作，经systemd-run延迟5秒调用systemctl。不放宽观测服务权限、不配置任意sudo命令。安装文件位于`/usr/local/lib/microduck-power-helper.py`及`/etc/systemd/system/`；启用socket后用power.conf添加环境变量。部署脚本保留旧release及标定备份，启动失败回滚current。

关机前托稳机器人；主板关机不切断舵机电源。等待约30秒并确认关机完成后再断电；页面断线本身不是关机完成证明。关机按钮需要重新供电才能再次启动主板。重启按钮提交后页面自动重连。接口和权限验证可用非法动作/缺确认请求，禁止将实际关机当作无影响的自动测试。

后端源码、环境变量与测试见[backend-rust](../backend-rust/README.md)。前端不变。旧Python服务和发布目录保留为回滚来源，旧版web-debug不修改。

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
