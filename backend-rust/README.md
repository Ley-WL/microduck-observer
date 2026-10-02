# Rust 后端

原生 Rust + Axum/Tokio，前端保持原样。运行时不启动 Python，不代理旧后端；HTTP `/api/v1/*`、WebSocket `/api/v1/stream`、protocolVersion=1 和共享标定文件格式沿用原版。

当前硬件入口为 MS901M UART、HD1910 TTL 同步读取和 tofd Unix Socket。BNO085 旧采集入口保留在 `debug-server/`，未迁移到 Rust；配置不支持的驱动会直接报错，不会静默使用模拟数据。

## 编译与启动

需要 Rust 1.87+。Linux 主控使用 ARM64 GNU 目标；Windows 可以开发模拟服务，生产 ToF 使用 Linux Unix Socket。

```bash
cargo build --release --locked
MICRODUCK_BIND=127.0.0.1:8877 ./target/release/microduck-observer
```

默认模拟模式，默认验证端口8878；前端默认连接8877，所以本地启动时显式设置上面的端口。前端开发命令保持 `cd frontend && npm run dev`。现有生产前端资源可以直接复用，无须重建。

| 环境变量 | 用途 |
|---|---|
| MICRODUCK_SOURCE | simulation / hardware |
| MICRODUCK_BIND | 监听地址，默认127.0.0.1:8878 |
| MICRODUCK_STATIC_DIR | 原前端dist目录 |
| MICRODUCK_CALIBRATION_FILE | 共享标定JSON，生产继续使用/var/lib/microduck-observer/calibration.json |
| MICRODUCK_IMU_DRIVER | ms901m |
| MICRODUCK_IMU_PORT | 当前HAT /dev/ttyS9，115200bps |
| MICRODUCK_SERVO_PORT | 当前HAT /dev/ttyS2，1Mbps；空值禁用 |
| MICRODUCK_SERVO_IDS | 逗号分隔的配置ID，生产为10–14、20–24、30–34 |
| MICRODUCK_TOF_SOCKET | /run/tofd/tof.sock；空值禁用 |
| MICRODUCK_POLICY_PATH | v5 ONNX模型，与同名.metadata.json配套 |
| ORT_DYLIB_PATH | 原生libonnxruntime.so路径，不需要Python解释器 |

## 控制与数据线程

IMU独立读取线程；舵机只有一个串口拥有者，采集、角度、使能、站姿、硬件中位和模型推理通过队列串行执行。WebSocket只复制各主题最新数据，不积压逐帧队列。原始反馈保存实际接收时间，发布与模型观察均按该时间计算年龄。

标定/运动共用任务锁，任务拥有锁而非浏览器连接拥有锁。全部失能可取消动作，失能后不再恢复目标；模型停止保持最后目标。硬件中位先持久保存备份，再执行3.46固件的0x0B指令；应答丢失不自动重试，逐颗保留结果并重新锁定EEPROM。

沿用[舵机当前规则](../docs/实测记录/03-舵机实测.md)：4.0–8.4V、官方角度范围、嘴部用户范围0–30°、加速度0/速度500；不写目标电流/PID，不新增速度/跟随/负载限制。站姿2.2秒过渡、3秒总期限、最终±5°持续100ms。发令统计独立于扫描与WS频率。

模型观测61维，动作14维；原生ONNX Runtime CPU单线程，严格检查模型SHA256、顺序与维度。安装方向、参考姿态、上一帧速度和原始上一动作的含义与Python版一致。输出按既有关节范围饱和。失败诊断包含原始回包、串口最近读写、IMU/关节参考、上次推理输入及卸力结果。

## 验证

```bash
cargo test --locked
python tests/contracts.py --reference ../debug-server
python tests/policy_fixture.py ../debug-server policy-fixture.json
MICRODUCK_POLICY_PARITY_FIXTURE=policy-fixture.json \
MICRODUCK_POLICY_PATH=../debug-server/models/hd1910-head-v5.onnx \
ORT_DYLIB_PATH=/path/libonnxruntime.so \
cargo test policy::tests::python_parity -- --ignored
```

HTTP/WS对比测试需要测试环境的FastAPI、Uvicorn、NumPy、Websockets；数值参考生成另需Python ONNX Runtime。这些只用于兼容性测试，不属于Rust服务运行依赖。测试启动两个隔离的模拟服务，使用临时标定文件，不访问硬件。

软件验证不代表实机运动验收。部署与只读性能结果维护在[平台实测](../docs/实测记录/04-网络与观测平台.md)及[HAT实测](../docs/实测记录/07-HAT实测.md)。
