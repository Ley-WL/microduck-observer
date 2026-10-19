# 金鸡独立 v62b 候选（复刻结构 / HD1910）

**模型身份：本项目自行训练的后抬右脚策略，采用 microduck-replica 的 STL/MJCF 与 HD1910（1910 M6）执行器参数。** 它不是 Hugging Face `RemiFabre/microduck-flamingo-cycle` 发布的 XL330 模型，也不是仅把上游模型的舵机名称改成 HD1910。

训练机器人入口为 `microduck-rl/src/mjlab_microduck/robot/replica/`，STL 源于 `microduck-replica/software/training/src/mjlab_microduck/robot/microduck/assets/`；执行器参数为 `robot/hd1910/1910_m6.json`，硬件配置名 `microduck-replica-hd1910-v1`。采用 HD1910 参数不代表实机动力学已辨识或动作已验收。

v62b 使用 v53 第 500 轮权重，没有新增优化器训练。`policy.onnx` 是原始已导出文件，SHA256 为 `21cc8fbf59d0edc5995145bea45bd4fedf8e928997ae66033dc1a21252fa48b2`。

左脚支撑、右脚后抬，入场最长等待 4.5 秒；progress=0.5 时先连续安静 15 帧，到位 progress=1 后再连续满足姿态条件 15 帧，才开始原有 150 帧（3 秒）保持。退出插值 1.5 秒、回站观察 2 秒，允许再次点击提前返回。61 输入、14 HOME 相对 raw 输出，50Hz，外部 EMA 旧 .45/新 .55 仅执行一次，raw action 历史。

## 验证边界

- 开发集完整周期 242/256（94.53%），未达到 95% 门槛。两个开发种子 111001、111007 各 121/128，已用于时钟选择，不是最终独立验收。
- 入场取消回站 127/128；保持取消回站 123/128，实际进入保持并触发取消 123 例。
- 仿真速度报告使用 window56 近似，返回行走倍率 .9；修正脚壳接触分类并把可视脚壳纳入完整离地判据，未移除任何物理碰撞。
- 此候选没有实机试验，也没有部署。此前板端版本三次实物失败仍有效。

## 当前 observer 兼容性

当前 Rust 时钟仍是入场 3 秒、单帧姿态就绪，因此不能把此 ONNX 直接覆盖 `rear_hook_native_gated.onnx` 并声称复现 v62b。本目录不参与默认模型发现，`runtimeKind` 使用未支持的候选名称，现有校验器会拒绝将其作为运行模型。配套 Rust 时钟尚未实现；未来启用前须补齐合同、独立验证和实机验收。

`evidence/` 保留服务器 manifest、4 组结果、comparison、时钟/评估/接触分类/速度报告器源码快照与原始测试结果。它们用于追溯，不是完整独立训练环境；完整实验来源见 manifest。`export-inputs.npz` 是 v53 导出时的 64 组输入/raw 输出，可用于 ONNX 一致性核验，不是新增成功率测试。
