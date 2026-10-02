# 2026-10-02 Rust 后端迁移验证

软件测试与主控静止只读实测；未启动站姿、模型动作、角度控制或硬件中位写入。

正式release：`20261002-rust-native-v2`，地址`192.168.31.193:8877`。systemd active/enabled，MainPID15567，对应ARM64原生Rust二进制；前端51个文件与原release一致，网页index与磁盘index SHA256一致。切换时15颗在线、扭矩均0且未改变；joints参考/方向、IMU参考/安装矩阵/目标姿态和安装位置逐项完全相等，revision133。浏览器沿用参考可能更新bootId/revision，不能将revision变化单独解释为标定丢失。

## 软件验证

- 26项Rust测试通过，包括寄存器写入顺序、真实电压边界、故障最终失能、未写电流/PID、模拟50Hz轨迹、标定保存和不确定EEPROM应答不重复写入。
- Python/Rust HTTP/WS对比通过：info、limits、poses、policy、标定读写/冲突/浮点精度、日志gap、订阅速率、消息字段、错误码与1008关闭。两端为临时模拟服务，不访问硬件。
- ARM64原生ONNX对比：观测最大绝对误差0；动作最大绝对误差`2.384185791015625e-7`，该次单次推理2.438ms。模型SHA256与元数据一致。该数据是固定合成输入，不能代替闭环性能。
- 原生发布包打包通过，不包含Python运行时、标定、密码或虚拟环境。ONNX动态库复用既有ARM64安装位置；进程不启动Python解释器。

## 同口径20秒只读采集

CPU为一个核100%的口径；Python包含主进程及采集子进程，Rust为同一进程所有线程。订阅joints/imu.raw/imu.orientation各50Hz、system1Hz。不同温度/频率及在线浏览器负载使此对比不能视为严格等频微基准。

| 指标 | Python切换前 | Rust正式服务 |
|---|---:|---:|
| 后端CPU，单核口径 | 153.28% | 46.02% |
| 实际串口扫描均值 | 48.64Hz | 49.69Hz |
| 舵机WS实际接收 | 31.28Hz | 49.42Hz |
| IMU四元数WS实际接收 | 31.82Hz | 49.77Hz |
| IMU raw WS实际接收 | 31.82Hz | 49.72Hz |
| 最老关节年龄P95 | 39.65ms | 15.91ms |
| 完整的已观察舵机帧 | 627/627 | 987/989 |
| 温度 | 85℃ | 83.89℃ |
| 采样起止CPU频率 | 1104MHz | 1800MHz |

Rust正式20秒有2个不完整的已观察帧，缺失4个行样本（14/20/21/24各1次），随后恢复；这不是“全程零漏包”或完整物理总线丢包率。上述约50Hz是采集与WS数据，不是实机运动发令统计。没有风扇、历史UART溢出、运动时供电/串口故障仍未验收解决。ToF/相机旧故障未恢复。

## 中间问题与回滚边界

首次隔离候选进程未带dialout附加组，串口Permission denied；测试脚本自动恢复Python服务，后按正式service权限启动候选。第二次发现默认JSON浮点解析导致旧标定HTTP数值末位不同，候选标定文件本身未写；启用float_roundtrip并加入精度回归对比，最终逐项相等。WS改为实际发送新样本后才推进节拍，舵机WS由首个候选约43Hz改善到约49Hz。

保留旧release `20261002-servo-scan-recovery`。主控部署备份、验证脚本及原service位于`/home/radxa/observer-rust-validation-20261002/`。回滚恢复current与service即可；保留当前共享标定，不自动覆盖新标定。

原始记录：[Python基线](python-before.json)、[Rust隔离候选](rust-candidate.json)、[Rust正式服务](rust-production.json)、[ARM推理对比](arm-policy-parity-v2.json)、[切换结果](switch-result.json)。
