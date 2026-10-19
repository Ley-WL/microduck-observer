# NPU实测

更新日期：2026-10-19。本文整理已留存的板端测试，没有为本次文档拆分重新跑分。后续NPU驱动、RKNN模型、检测前后处理及推理性能统一维护于此；原始附件保留原路径。

## 当前结论

| 项目 | 实测状态与边界 |
|---|---|
| RK3566 NPU | 已启用，重启后驱动绑定，原生RKNN推理成功 |
| MobileNet V1 | 官方狗图冒烟通过；20次计时均值5.381ms，不含读图解码 |
| YOLOv5n人体检测 | INT8模型可运行，官方图检出3人；摄像头实拍未检出 |
| RetinaFace Mobile320 | 量化模型可运行，官方图检出1脸；存在FP16张量和CPU算子，实拍未检出 |
| RGA检测预处理 | 独立基准已使用硬件缩放，缩放耗时下降；尚非零拷贝 |
| 常驻检测 | 尚未接入Observer，未验收连续视频检测或叠框 |
| 摄像头硬件编码 | MPP/RGA链路已上线，属于视频处理；详见[摄像头实测](05-摄像头实测.md)，不代表NPU检测已上线 |

板端测试使用原生C/C++和RKNN Runtime，没有Python推理服务。Python仅用于PC端模型转换等开发操作。现有机器人运动策略仍走CPU，本记录不代表运动模型已迁移NPU。

## 设备与版本

| 项目 | 本次测试配置 |
|---|---|
| 设备 | Radxa ZERO 3 / RK3566，`microduck-e6fc56` |
| 最近测试地址 | `192.168.137.224`，热点DHCP地址可能变化 |
| 系统 | Armbian 26.2.1 / Debian 13，内核6.1.115-vendor-rk35xx |
| RKNPU驱动 / Runtime | 0.9.8 / 2.3.2 |
| PC转换工具 | RKNN Toolkit2 2.3.2 |
| Toolkit2固定版本 | `42aa1d426c0a9e0869b6374edba009f7208a1926`（v2.3.2） |
| Model Zoo固定版本 | `bad6c7334531becaf90a561988519b7bec34d0ab`（v2.3.2） |
| RGA驱动 | RGA2 1.3.7 |
| NPU频率采样 | 900MHz；未锁CPU/NPU/DDR频率 |

测试期间Observer及摄像头运行，舵机在线卸力。初期温度78.1～82.8℃，后续约84～85℃；不同轮次存在温度、频率和系统调度差异，不宜跨轮次直接认定性能回归。

## 2026-10-19 驱动启用

内核已有`CONFIG_ROCKCHIP_RKNPU=y`，原设备树NPU与IOMMU节点为disabled。[设备树覆盖](../../deploy/radxa-zero3-npu-enable.dts)仅把`/npu@fde40000`和`/iommu@fde4b000`设为okay，未修改时钟、电压或OPP。

安装位置为`/boot/overlay-user/microduck-npu-enable.dtbo`，启动配置追加`user_overlays`；原文件备份为`/boot/armbianEnv.txt.before-npu-20261019-123301`。卸力后重启，确认驱动绑定、IOMMU启用，`/dev/dri/renderD129`对应NPU，debugfs存在`/sys/kernel/debug/rknpu/{version,load,freq}`。恢复启动配置备份并在卸力后重启可撤销启用。

启动日志有mem-supply缺失、can't request region、IRQ npu_irq not found提示，随后初始化成功，之后真实推理通过；这不能证明所有模型和长期运行均不受影响。[启用证据](附件/主板/2026-10-19/npu-enable/verification.json)，原始过程见[主板记录](01-主板与接线.md)。

## 2026-10-19 MobileNet冒烟

官方`mobilenet_v1.rknn`为4,470,313字节，输入RGB 224×224、输出1001类。[原生测试程序](../../tools/rknn-smoke.c)使用UINT8/NHWC输入，沿用模型内置预处理，不额外归一化。

共23次调用成功，3次预热、20次计时：均值 **5.381ms**、最小5.127ms、最大6.001ms。计时包含inputs_set/run/outputs_get，排除JPEG解码、模型初始化和结果排序。每次输出1001个有限值；Top1为156/Shih-Tzu，分数0.927734，与官方狗图相符。中断0→23、退出码0、stderr为空，支持硬件推理链路可用结论。缺参数返回2、缺图片返回1也已验证。

板端复现（测试文件仍需存在，机器人保持卸力）：

```sh
sudo runuser -u radxa -g radxa -G render -G video -- env LD_LIBRARY_PATH=/home/radxa/npu-smoke timeout 30 /home/radxa/npu-smoke/rknn-smoke /home/radxa/npu-smoke/mobilenet_v1.rknn /home/radxa/npu-smoke/dog_224x224.jpg
```

此次附加组仅作用于测试进程，未改用户长期组成员。模型及库位于主板`/home/radxa/npu-smoke`和本地忽略目录`.local/rknn-smoke`，未覆盖系统库。[运行结果](附件/主板/2026-10-19/npu-smoke/result.json)、[来源与SHA](附件/主板/2026-10-19/npu-smoke/manifest.json)。这是单图冒烟，不是分类精度、功耗或长期稳定性验收。

## 2026-10-19 轻量检测模型与实拍

| 模型 | 输入 | RKNN大小 | 量化与校准边界 |
|---|---:|---:|---|
| YOLOv5n | 640×640 | 3,046,136字节 | INT8输入及3个输出；官方COCO 20图校准；仅过滤展示person，不减少网络计算 |
| RetinaFace Mobile320 | 320×320 | 643,811字节 | INT8量化模型，含FP16输出及CPU算子；仅官方test.jpg校准，测试同图不是独立精度验证 |

构建器曾打印`Unkown op target: 0`，build/export成功，随后板端执行通过。[模型准备与哈希](附件/摄像头/2026-10-19/vision-detectors/preparation.json)。

以下为最初CPU缩放基线，每组5次预热、50次重复同图。pipeline包含缩放、RKNN输入/运行/输出及后处理，排除读图解码、旋转、采集、网络和显示。CPU为检测进程占单核比例，RSS为进程峰值。

| 模型/输入 | pipeline均值 / P95（ms） | rknn_run（ms） | CPU单核 | RSS（MiB） | 结果 |
|---|---:|---:|---:|---:|---|
| YOLO / 官方bus | 60.458 / 62.742 | 26.011 | 56.01% | 28.68 | 3人 |
| RetinaFace / 官方test | 13.502 / 14.091 | 6.842 | 73.39% | 11.82 | 1脸 |
| YOLO / 第二张实拍360×640 | 47.525 / 49.072 | 25.632 | 44.80% | 28.23 | 0人 |
| RetinaFace / 第二张实拍360×640 | 13.024 / 14.098 | 6.962 | 71.63% | 11.85 | 0脸 |

官方图框已目视核对。两次实拍均明显欠曝、仰拍且脸不完整，用户调整光线后仍未检出。第二帧离线gamma=3提亮后两模型仍为0，暗区缺失细节；实拍效果本轮未通过，不能只凭官方图成功宣布检测可用。私人实拍仅保存在本地忽略目录。

证据：[官方图](附件/摄像头/2026-10-19/vision-detectors/official-report.json)、[第二张实拍](附件/摄像头/2026-10-19/vision-detectors/live-report.json)、[提亮对照](附件/摄像头/2026-10-19/vision-detectors/bright-report.json)。网络恢复后已完成板端测试，原“等待网络恢复”段落只是历史状态。

## 2026-10-19 RGA预处理对照

[构建脚本](../../tools/build-rknn-detect-bench.sh)支持`USE_RGA=1`，基准复用官方预后处理。生成的image_utils副本仅将RGB888宽度步长约束从16放宽至4像素，使360像素宽输入可用RGA，其它格式保留约束。测试程序仍有显式记录的CPU缩放回退路径；本次RGA样本未触发回退。这与生产摄像头取消软件编解码是两个不同范围。

最终相邻CPU→RGA对照为每组5次预热、50次同图，摄像头持续运行，两组均以root测试。此时未持久修改设备权限；后来摄像头MPP部署的video组权限配置见05。

| 项目 | YOLO CPU→RGA | RetinaFace CPU→RGA |
|---|---:|---:|
| pipeline均值（ms） | 67.096→46.396 | 22.507→15.030 |
| pipeline P95（ms） | 104.239→53.971 | 25.260→16.557 |
| 缩放均值（ms） | 33.612→7.557 | 10.745→3.107 |
| rknn_run均值（ms） | 26.691→28.972 | 10.772→10.843 |
| CPU单核 | 60.46%→34.69% | 81.62%→65.17% |
| RSS（MiB） | 28.24→28.49 | 11.85→12.09 |

该轮pipeline分别减少30.9%/33.2%，不是固定收益。此前另一轮人脸17.812→17.590ms，运行阶段波动几乎抵消缩放收益，原始结果同样保留。官方bus图CPU/RGA均3人且框/分数一致；人脸均1脸，左边界相差2像素。8组退出码、回退检查和样本框偏差校验通过，但不是数据集精度验收，也未实现零拷贝。

```sh
USE_RGA=1 sh tools/build-rknn-detect-bench.sh .local/vision-bench/zoo .local/rknn-smoke .local/vision-bench/rga-stage
```

板端库位于`/home/radxa/vision-bench`及`/home/radxa/npu-smoke`，未全局替换。[完整8组](附件/摄像头/2026-10-19/vision-rga/rga-report.json)、[相邻复测](附件/摄像头/2026-10-19/vision-rga/rga-paired-report.json)、[结果校验](附件/摄像头/2026-10-19/vision-rga/validation.json)、[库与二进制SHA](附件/摄像头/2026-10-19/vision-rga/manifest.json)。

## 2026-10-19 最新分段耗时与CPU归因

[检测基准源码](../../tools/rknn-detect-bench.cc)使用`CLOCK_PROCESS_CPUTIME_ID`测进程全部线程的CPU时间。本轮RGA路径5次预热、100次同一360×640实拍，关闭逐层诊断；与上表是不同轮次。

| 阶段 | YOLO墙钟 / CPU（ms） | RetinaFace墙钟 / CPU（ms） |
|---|---:|---:|
| RGA缩放/填边/缓冲映射 | 8.486 / 4.227 | 4.442 / 2.440 |
| inputs_set | 2.191 / 2.024 | 0.682 / 0.639 |
| rknn_run | 29.765 / 5.405 | 15.947 / 11.310 |
| outputs_get | 8.930 / 6.439 | 0.862 / 0.846 |
| 后处理与其它余项 | 0.375 / 0.433 | 0.151 / 0.200 |
| 整帧墙钟 / CPU近似合计 | 49.748 / 18.528 | 22.085 / 15.435 |

CPU时间不等于墙钟等待时间；采样开销使分项和略有差异。“其它”包含分配、释放和日志，不能视为纯NMS。无间隔循环CPU占单核37.28%/69.95%。按CPU-ms估算各10Hz约18.5%/15.4%、合计33.9%单核，仅是调度预算，尚未实测双模型连续视频共存。

**rknn_run不是纯NPU时间，量化模型也不意味着全算子INT8或全算子在NPU。** 单独启用`RKNN_BENCH_PERF=1`后，RetinaFace显示5个CPU Transpose、部分CPU Reshape及FLOAT16 CPU exSoftmax13。该诊断算子表CPU合计3.820ms、NPU8.786ms，Transpose2.118ms、Softmax1.252ms。它有诊断开销，不能与正常循环直接相减或解释全部CPU时间。

YOLO主要算子在NPU，诊断OutputOperator在CPU、单次3.010ms；其输出为2,142,000字节INT8。输出布局、缓存和拷贝值得继续分析，尚不能把outputs_get的8.930ms全部归因于拷贝。比较官方推理速度时须对齐计时范围，不能拿包含前后处理的pipeline与仅模型运行的FPS直接比较。

证据：[正常分段统计](附件/摄像头/2026-10-19/vision-profile/stage-cpu-report.json)、[RetinaFace逐层](附件/摄像头/2026-10-19/vision-profile/perf-face.log)、[YOLO逐层](附件/摄像头/2026-10-19/vision-profile/perf-yolo.log)。

当时系统采样为CPU 1.104GHz、DDR 1.056GHz、NPU 900MHz、约85℃。检测之外的Observer含舵机/IMU读取开销，不应全部归相机或NPU。[当时系统基线](附件/摄像头/2026-10-19/vision-profile/system-profile.json)、[线程采样](附件/摄像头/2026-10-19/vision-profile/observer-threads.json)属于旧FFmpeg摄像头窗口；后续MPP替换后的资源变化以[05当前记录](05-摄像头实测.md)为准，尚未在新摄像头链路下重测双模型。

## 未完成项与后续记录口径

1. 改善曝光和取景，用独立、多场景、连续帧验证人体/人脸检出效果；扩大校准集，不能用同一校准图代替精度集。
2. 研究直接消费NV12/共享帧，用RGA完成旋转、缩放和颜色转换，避免检测侧JPEG往返；目前只是方案。
3. 优化YOLO输出布局与缓冲复用；调整RetinaFace导出图中的CPU算子后重新量化、回归坐标和分数。
4. 完成常驻检测调度、资源权限和Observer接入后，实测双模型与摄像头共存的CPU、内存、丢帧及端到端延迟。
5. 在可复现的温度和频率条件下补长时测试。现有重复静态图数据不代表视频实时FPS、整机功耗或长期稳定性。

新增实测须注明设备、日期、模型哈希、量化方式、输入、预热/样本数、计时边界、温度/频率及后台负载，同时记录失败与未验证项。附件仍可引用原目录；不提交凭据、私人实拍、第三方模型和构建二进制。
