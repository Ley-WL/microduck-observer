# 2026-09-23 HD-1910 只读采集与推理验证

硬件：Radxa 主控（aarch64）、15 个 HD-1910 舵机、1 Mbps 串口、BNO085 IMU。
所有测试未发送扭矩使能、位置目标、运动或舵机标定写入指令。

| 文件 | 阶段 | 时长 | 主要结果 |
|---|---|---|---|
| chunk-read-benchmark.json | 同进程成块串口读取 | 8 s | 124 组观测，最老关节 P95 131.41 ms |
| process-poller-benchmark.json | 独立进程，逐颗 READ | 10 s | 306 组观测，扫描 P95 34.20 ms |
| sync-poller-benchmark.json | 独立进程，SYNC_READ，原发布周期 | 10 s | 源发布 44.05 Hz，扫描 P95 9.06 ms |
| final-poller-benchmark.json | SYNC_READ + 10 ms 消费周期 + 异步快照接口 | 30 s | 源发布 49.36 Hz，最老关节 P95 30.53 ms，零请求异常、零缺失舵机行 |
| board-inference-first.json | 主控 ONNX 合成基准及首轮真实输入检查 | 30 s 真实输入 | 7 个模型加载成功，合成输入 P95 0.89–0.94 ms；真实输入 0 成功 / 1471 拒绝，不是实时推理通过证据 |

`unique_joint_frames` 是快照采样实际看到的不同帧数，不能直接等同源发布频率；`source_frame_hz` 根据源序号及源单调时间计算。`servo_read_ms` 在同步读取阶段表示一次批量请求发出到各 ID 回包解析的时间，和逐颗 READ 的往返时间含义不同。

`request_ms` 是主控 localhost HTTP 请求耗时；`joint_oldest_ms` 是快照生成时最老关节的数据年龄，不包含后续网络传输。只读推理全链路仍需单独验证；本目录不声称动作效果通过。

复测：

```bash
python scripts/measure_observer_latency.py --endpoint http://127.0.0.1:8877 --seconds 30 --output /tmp/observer-latency.json
python -m unittest discover -s debug-server -p 'test_*.py'
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 python -m pytest tests/test_shadow_policy.py -q
npm --prefix frontend test
npm --prefix frontend run build
```

提交前测试：后端 26、影子推理 8、前端 23，共 57 项通过；前端构建通过（已有大 bundle 提示）。推理依赖实际版本见 requirements-installed.txt，环境和 ONNX 权重未入库。
