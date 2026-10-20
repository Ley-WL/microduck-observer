> 2026-10-20 current: full 3280×2464 sensor/ISP crop → hardware-scaled 640×480, no crop. Nominal 21 fps capture, one in two frames → 10.5 fps preview; 480×640 upright, v2 RGB IPC (921600 bytes). Deployed on 192.168.137.238; earlier 360/480 notes below are historical.

# 原地看人互动运行链路

第一阶段只注视、寻找和叫声回应，不包含招手识别、麦克风、身体移动、舞蹈或跟随。Rust Observer 管理摄像头与 C++ RKNN helper，仍由唯一串口线程执行头部目标。无 Python 运行服务。

## 图像与推理

现有 ISP NV12 → RGA RGB → 原固定白平衡/gamma LUT → 最新 RGB 槽。预览继续走 RGA NV12 → MPP JPEG。检测直接读取校正后的 RGB，不经过 JPEG 编码/解码；RGA 顺时针旋转 90° 成 480×640，再硬件缩放、填边，调用既有 YOLOv5n INT8 和 RetinaFace Mobile320 RKNN 模型。

`vision-frame.h` 定义一个 32 字节头（v2 magic、序号、CLOCK_MONOTONIC ns、字节数）与 921600 字节 RGB。文件位于 systemd RuntimeDirectory，0600，flock 防止撕裂。相机写端非阻塞，争用则丢帧；只保留最新帧。无消费者时不复制 RGB。不是 DMA 零拷贝：采集管道、最新帧槽和 RKNN 输入/输出仍有内存复制。RGA 失败明确报错退出，不回退 CPU 缩放。

helper 标准输入每次只接受 `p`（仅人体）或 `b`（先人体，有人体才运行人脸）；旧 `f` 独立人脸请求被拒绝。标准输出每次一行 JSON，最多 64 框。模型常驻，每次最多一个请求。搜索起始间隔 200ms，人体锁定后 100ms，推理计入间隔、不积压补帧，活动轮询 10ms。人体始终检测；仅当当前人体框连续至少 500ms 稳定时请求人脸细化。稳定判据是相对锚点四边变化均小于画面尺寸 3%，有效人体帧间隔不超过 300ms；累计缓慢漂移也会重置计时。此为画面稳定估计，不是世界坐标运动判定，转头/遮挡也可能重置。请求依据上一帧，当前帧若已移动则 Rust 丢弃人脸细化结果。人工接管/起身/退出时回收 helper；摄像头临时使用与持久预览设置分离。

RetinaFace 图内仍有 CPU Transpose/Reshape/Softmax；白平衡 LUT、框解码/NMS、目标选择、状态机和 IPC 仍使用 CPU。`frameIntervalMs`（采集帧间隔，上限 300ms）、`totalMs`、`cpuMs`、`rotationMs`、`resizeMs`、`faceMs`、`personMs`、`maxRssKiB` 用于实测。模型阶段含其前后处理，resizeMs 是子集，不应重复求和。帧时间戳在校正 RGB 发布时记录，不是传感器曝光时间。Rust 用同一单调时钟重新计算结果年龄，避免 IPC 延迟伪装新帧。

## 控制与失效

有效框需类别、分数、有限坐标与边界检查。只从人体框选择中央目标，同一人体连续约 500ms 后注视。持续以人体关联，稳定后只接受中心位于该人体框内且处于上部 60%（兼容近景只检出上半身） 的人脸；人脸不参与首次锁定或人体丢失续期。缺脸、移动时使用人体横向中心/高度上部 22% 估算头部位置。不缓存旧脸位置覆盖新人体估算。多人交叉仍可能交换，不做人脸身份识别。

目标误差有 6% 死区，头部偏移俯仰 ±18°、偏航 ±35°，速度上限 36°/s（50Hz 控制每步 0.72°），归一化误差增益俯仰 60、偏航 90°/s，再经过已有标定和机械/编码限位。目标丢失保持 2s 后进入随机观望：相对站立模型姿态抬头 10°（机械偏移 −10°），左右交替随机目标幅度 12～32°，每段速度 20～34°/s，到位停顿 0.2～0.65s；80°/s² 加减速并在端点前制动，最终仍经过 36°/s 和机械限位。首次收到有效无人体画面即进入观望，发现人体立即撤销扫视、接回估算/人脸注视。相机帧超过 600ms 或处理错误则不搜索、不沿旧框追踪。输入结果年龄超过 500ms 拒绝。正常身体指令始终为零。

首次锁定后单声短叫；重新发现冷却 20s，同一目标停留至少隔 28s 轻叫一次，现有声音忙时不覆盖。人工输入取消互动声音。手柄优先、回中恢复、跌倒总 6s 恢复规则不变。开启/正常退出自主均为两声短叫。方向、实际转头效果与光线适应仍需板端现场验证。

## 构建和打包

使用已有 Zoo v2.3.2 固定源码 `bad6c7334531becaf90a561988519b7bec34d0ab`、RKNN SDK 2.3.2、MPP/RGA 库，不下载或替换全局库。参数为实际本地依赖路径；以下为本工作区复现命令（WSL）：

```sh
sh tools/package-vision-runtime.sh .local/vision-bench/zoo .local/rknn-smoke .local/mpp .local/mpp-bin .local/vision-bench .local/vision-runtime
g++ -std=c++17 -O2 -Wall -Wextra tools/test-vision-frame.cc -o .local/test-vision-frame
.local/test-vision-frame
cd backend-rust
cargo test --bin microduck-observer
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-gnu
```

打包器验证两模型 SHA 后生成候选 `bin/`、`lib/`、`models/vision/`、`vision.conf` 与 `SHA256SUMS`，不安装或重启服务。部署时保留既有全部运动模型/配置，把这些增量与新 Observer、网页放入新的版本目录，并安装 vision.conf 到主服务 drop-in（RuntimeDirectory、render/video 组、DRM 设备权限）。先确认全 15 舵机新鲜反馈且卸力；保留原 current 和旧 drop-in 可回退。

## 待板端验收

先全卸力验证官方图片 RGA/RKNN 输出与此前 CPU/RGA 基准一致，再仅感知读取真实最新帧，记录帧序号、旋转方向、框、CPU 时间和峰值 RSS。随后由用户现场站立开启自主，验证中央/左右/上下、多人、丢失/重新出现、手柄抢占、相机失联和退出释放。分别统计仅预览、搜索、注视三个负载，以及运动控制周期变化。不得用离线状态机测试代替实拍检出率、头部方向或机器人稳定性验收。

2026-10-19 开发时原 SSH 192.168.137.178 超时，已请求当前地址。硬件调用尚未在本轮板端执行，CPU/RSS/FPS 与实拍效果待测；旧记录中的暗光实拍未检出问题不能视作已解决。

2026-10-20：121 已部署 v1，RGA/RKNN 官方图与真实画面验证通过。用户手动站立开启后反馈慢速可跟随、快速跟不上。v2 已部署上述调度和响应参数，卸力态独立探针有效更新约 8.2/8.6fps、检测单核 CPU 10.19%/7.64%，快速物理跟随尚待复测；实测见 docs/实测记录/08-NPU实测.md 顶部。

2026-10-20 后续 v3：已部署人体优先流程，258 Rust/ARM/native、板端固定图分支与卸力发布验证通过。实时负载采样无人检出，人脸被跳过；不能当作稳定有人时的双模型负载。实物移动/停下/遮脸互动待用户验收，记录见 08-NPU实测 顶部与 vision-body-first 附件。

2026-10-20 随机观望已部署 runtime-20261020-vision-interaction-v4b：260 Rust测试、ARM及板端模型/边界/卸力发布验证通过，具体参数见上文。未主动开启自主或驱动舵机，现场效果待用户确认。

2026-10-20 分辨率最新覆盖：已部署640×480采集/480×640旋转后预览与检测，10fps目标。此前360×640及691200字节只属于历史版本。当前v2 IPC为921600字节；见 vision-480 附件实拍与发布验证。

2026-10-20 最新恢复：发现1920×1080输入未经裁剪被缩放为640×480造成形变，用户选择完整视野，现已恢复v4b/640×360及v1 IPC。实际驱动枚举模式为1920×1080@30fps、3280×2464@21fps；未切传感器模式。此前480试用记录仅为历史。
