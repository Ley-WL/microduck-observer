# 观测协议 v1 · 当前实现

此协议独立于运动控制。所有数据使用 JSON；服务主版本 `protocolVersion: 1`。

## HTTP

| 方法与路径 | 含义 |
| --- | --- |
| GET `/api/v1/info` | name、bootId、protocolVersion、source、topics、capabilities |
| GET `/api/v1/health` | 存活状态、场景、日志缓存条数 |
| GET `/api/v1/snapshot` | 按主题组织最新样本，保留原 seq 和采样时间 |
| GET `/api/v1/logs?cursor=0&limit=200` | seq 大于 cursor 的日志；返回 items、nextCursor、gap；limit 为 1–2000 |
| POST `/api/v1/debug/scenario` | 仅模拟器提供；请求 `{"name":"motion"}` |

场景为 motion、steady、imu_pause、invalid、log_burst。debug 接口不属于未来实机的必需接口，前端根据 `capabilities.scenarios` 显示按钮。

## WebSocket

连接 `/api/v1/stream` 后 5 秒内发送一次订阅：

```json
{"type":"subscribe","requestId":"dashboard","topics":{"pose":50,"imu.raw":50,"system":1,"logs":null}}
```

服务返回 `type: subscribed`，含 requestId、protocolVersion、bootId 和实际订阅 topics。未知主题不会出现在确认中；姿态和 IMU 上限 50 Hz，system 上限 1 Hz，logs 为事件驱动。当前版本改变订阅需重新连接。

每 2 秒推送 `{"type":"heartbeat","bootId":"..."}`。浏览器超过 6 秒没有任何服务消息时关闭连接并重试。慢客户端单次发送超过 2 秒即断开；遥测使用最新状态缓存合并，日志每批最多 100 条，服务日志缓存最多 2,000 条。

```json
{
  "type":"sample", "protocolVersion":1, "bootId":"run-uuid",
  "topic":"pose", "seq":42, "sampleMonoMs":1000, "ageMs":2,
  "source":"simulation", "valid":true,
  "data":{"frame":"robot","quaternion":[0,0,0,1],"calibrationId":"simulation-identity"}
}
```

seq 在 bootId 内按主题递增，保留样本不能改 seq。sampleMonoMs 使用服务单调时间轴，ageMs 是发送时样本年龄。姿态 quaternion 为 xyzw、身体到世界的单位四元数，右手系 X 前、Y 左、Z 上。安装坐标变换由数据源负责。

`imu.raw` 的 data 包含 frame、gyro（三轴 rad/s）、accel（三轴 m/s²）。模拟 accel 包含重力对应的静态比力，未声称是去重力线加速度。

`system` 的 data 包含 uptimeSeconds 和 scenario。当前不伪造 CPU、电压、电量等实机指标。

日志通过 `{"type":"log_batch","items":[...],"gap":false}` 推送；每条 item 是 topic=logs 的 sample，data 包含 level（DEBUG/INFO/WARN/ERROR）、module、message、eventTimeMs（Unix 毫秒）。缺失历史时 gap=true。前端同样检查日志 seq 连续性。重连从服务仍保留的历史开始推送，前端按 seq 去重；新 bootId 清空旧历史。

source 可为 simulation、hardware、replay。服务根据 `MICRODUCK_SOURCE=simulation|hardware` 选择模拟或实机模式，实机异常时不会自动退回模拟。数据无效或超龄时不得作为实时测量展示。相机、ToF、关节和录制协议待后续实现。

## v0.2 实机模式

实机 `info` 声明 `pose: false`、`sensorOrientation: true`、`scenarios: false`。提供 `imu.orientation`、`imu.raw`、`system`、`logs` 主题，未校准时不产生 `pose`。

`imu.orientation` 的 data 为 `frame: sensor`、`quaternion: [x,y,z,w]`、`accuracy: 0..3`（SH-2 融合精度标记）、`mountingCalibrated: false`、`calibrationId: null`。融合精度和安装校准是不同概念。四元数将传感器坐标映射到 IMU 参考系，不能直接视为机器人身体坐标。

实机样本 `timestampBasis: host_receive`，时间取自主控接收到报告的单调时钟，尚未重建 SH-2 设备采样时间。`imu.raw` 在收到新角速度后组合最近加速度，时间采用两者中较早值；加速度超过 500 ms 未更新即标记无效，不会被新角速度刷新成新鲜数据。

`health.imu` 和 `system.data.imu` 包含 device、address、productId、state、sampleAgeMs、counts、observedHz、ioErrors、unparsedReports、droppedEvents、mountingCalibrated。observedHz 是服务采集线程启动以来平均报告率；浏览器另行计算自身接收率。
