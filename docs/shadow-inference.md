# HD1910 主控只读推理验证

入口：`scripts/shadow_policy.py`。它仅 GET 观测台 `/api/v1/snapshot`，无串口/I2C 驱动、无运动 API、无扭矩使能或目标写入路径。输出是诊断日志，不能用来驱动机器人。

## 主控运行

```bash
cd /home/radxa/microduck-shadow
./venv/bin/python shadow_policy.py --models models --geometry model.json --seconds 30 --output results/manual-check
```

先对目录内每个 ONNX 做 20 次预热及 200 次固定合成观测推理，记录 p50/p95/max。随后使用站立模型进行有时限的真实观测影子推理，目标循环 50 Hz。循环包括 HTTP 时间，不能用纯推理耗时替代端到端频率。

`summary.json` 记录合成基准与真实输入成功/拒绝次数；`shadow.jsonl` 保存原始编码器步、角度、速度、61 维输入、14 维预测、推理耗时。每条记录 `executed=false`。未注册开机启动或后台常驻任务。

## 输入约定与局限

- 从 ONNX metadata 读取关节顺序、默认角度和观测名称；要求 `[1,61] -> [1,14]`，使用已包含归一化器的导出模型。
- 关节 ID、轴和限位来自观测台 model.json；用户确认标定时实物对应网页参考姿势。模型默认角度 metadata 为三位小数，存在最多约 0.0005 rad 的舍入误差。
- 第 15 个嘴舵机不在策略输入内。编码器原值不更改；只选择关节限位内唯一的整圈等价角度。找不到或不唯一则拒绝推理，不裁剪成有效姿态。
- IMU 使用主板保存的本次启动参考四元数与用户确认的 -90° 安装方向，计算相对姿态投影重力；角速度同样转换轴向。当前只是经过人工方向核对的显示标定，尚非完整物理安装矩阵测量。
- 关节速度由每颗舵机读取时间及角度差分估算，舵机采集异步且低于策略频率。重复观测保持上一速度；时间倒退、长间隔、异常大速度拒绝推理。
- 150 ms 新鲜度门限用于诊断，并不证明满足实机控制时延；离线、故障、缺标定、非有限数、IMU 启动不匹配均阻止本帧推理。
- 命令槽全零，仅验证站立。上一动作槽填上一帧预测，而没有真实执行反馈，因此这不是闭环，也不能证明能站稳、防摔或行走。
- 只读运行停止不会改变现有舵机状态。现有观测台进程独立，模型环境也与观测台隔离。

## 测试

```bash
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 python -m pytest tests/test_shadow_policy.py -q
```

包含唯一整圈分支、歧义/越界、轴向、61 维布局、速度换算及传感器失效拒绝用例。主控运行依赖 NumPy 和 ONNX Runtime CPU；实际版本在验证结果目录的 requirements-installed.txt 中记录。环境、模型和运行日志不提交到 Git。

## 2026-09-23 首轮结果

主控 aarch64 / Python 3.13.5，独立环境 NumPy 2.5.3 / ONNX Runtime 1.30.0。7 个模型均完成合成输入测试：P95 0.89–0.94 ms，单次最大 1.26 ms 以内。这个结果只覆盖 CPU 模型计算。

30 秒真实输入测试：0 帧成功、1471 帧拒绝。原因包括 22 号左髋俯仰换算 -1.709 rad，超出模型 ±π/2；部分关节/IMU 帧超过 150 ms 门限。不能据此声称完成真实姿态策略验证；需要摆回参考姿势，再测新鲜度和输出。程序已正常结束，未留后台运行。

本地报告：`outputs/hd1910-shadow-20260923/board-first/summary.json`。
主控报告及逐帧日志：`/home/radxa/microduck-shadow/results/20260923-first/`。

## 2026-09-23 参考持久化更新

观测台现在跨服务会话恢复已保存的 IMU 参考用于显示，并标注待核对。用户确认实物与显示一致后点击“确认沿用已存参考”，保留四元数和原标定时间，仅确认当前会话；不采集新零位。方向不一致则需要重新归零。当前驱动不能可靠识别所有传感器断电/内部重置，不能将此功能宣称为自动识别参考有效性。

安装方向支持 -90/0/+90 度，保存至主板独立 mounting.yaw 字段，更新不会修改 joints 或 imu 参考。影子推理读取该字段，不再固定 -90 度；仍拒绝跨会话未确认参考。缺少新字段的旧标定默认 -90 度，与原显示修正一致。

验证：22 项后端、23 项前端、8 项影子推理测试通过；实际部署重启后核对 15 项舵机标定及 IMU 四元数均未变。浏览器确认恢复提示与沿用按钮可用，未代用户点击确认。环境无新增依赖。


## 仓库内复现

在具有 NumPy、ONNX Runtime 的独立环境运行；无需在观测台服务环境安装模型依赖。已测试版本见 `validation/20260923/requirements-installed.txt`（主控 Python 3.13.5）。模型需自行提供带有归一化器和 metadata 的 61D/14D 导出文件，未将模型权重或运行环境上传至本仓库。

```bash
python scripts/measure_observer_latency.py --endpoint http://127.0.0.1:8877 --seconds 30 --output /tmp/observer-latency.json
python scripts/shadow_policy.py --models /path/to/models --geometry frontend/public/model/model.json --seconds 30 --output /tmp/shadow-check
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 python -m pytest tests/test_shadow_policy.py -q
```

采集优化最终报告：`validation/20260923/final-poller-benchmark.json`。首轮推理报告 `board-inference-first.json` 保留真实输入被拦截的结果，不代表实时推理已通过。性能数据来源于一次对应时长的现场测试，不是长期稳定性保证。
