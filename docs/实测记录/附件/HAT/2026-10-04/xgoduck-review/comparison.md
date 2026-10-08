# XgoDuck训练/Arduino部署与当前MicroDuck对照（2026-10-04）

仅源码审查和CPU仿真，不发实物动作，不写PID/EEPROM/标定，不换模型。GitHub读源使用固定HEAD：训练326d77a1122870bdefa2c36403937502c958e69c，部署8cdbbd84710d856581982c9eaf0d5e2970666232。用户提供本机7890代理，git请求验证成功；源码API/raw固定SHA留存于sources/。

| 项目 | XgoDuck | 当前平台/模型 | 意义 |
|---|---|---|---|
| BAM识别参数 | 1910_m6.json SHA ef2d51ad… | 完全相同SHA | 不是缺少该型号参数文件 |
| 训练增益 | kp_fw=5 | kp_fw=5 | 仿真基准一致 |
| 实机增益 | ARM写临时50/51：P=6/D=20，嘴P=10；README存储P=5/D=20 | 最近完整反馈50/51/52：P=32/D=40/I=0；平台保持不写PID | 运行参数差异最值得先量化；临时/存储不能混淆，不能把原始P直接等同BAM增益 |
| 动作 | HOME+action，scale1，50Hz；EMA旧0.45+新0.55 | HOME+action，scale1，50Hz；无动作EMA | 平滑改变闭环，须作为模型运行配置经对照验收；prev_action应继续为原始网络输出 |
| 反馈 | MCU100Hz，0x82读56起6字节；速度读寄存器，低通旧0.4+新0.6 | 单串口owner50Hz，56起15字节，保留电压/fault/current；模型速度用上一帧位置差分 | 可对照寄存器速度及滤波，不能删有效故障字段或凭代码报告100Hz实测 |
| IMU | QMI8658加速度/gyro互补重力tau0.3秒；gyro低通0.5 | MS901M四元数重力+共享安装矩阵，gyro现值 | 传感器不同，不照搬轴置换或加速度融合 |
| 上下文 | 61→14，原始prev_action，归一化内嵌 | 同语义 | 基本契约一致，无证据需改维度/scale |
| HOME | 训练髋/踝±24°，部署±23°，头颈20° | v5髋约26.24°/踝25.95°，头颈20°；运行读取模型metadata | 对方自身训练/部署HOME约1°差异；不要照抄其23°或固定HOME |
| 几何与质量 | XgoDuck CAD、0.7999999893kg、HOME trunk Z0.139439m | replica CAD、0.73724318kg、HOME Z0.11718236m | 同舵机不等于同模型；重训应核对实际装配质量/惯量/足接触 |
| ID映射 | 模型左腿映射10–14、右腿20–24 | 当前模型左腿20–24、右腿10–14 | 是各自实物映射配置，不能仅因数字不同认定我们接错 |
| 缺反馈 | 包里有servoMask；常规policy有效条件未逐颗校验mask/年龄，可带旧关节；host150ms/IMU100ms/command250ms停机 | 逐颗真实年龄、3帧coast/150ms、持续缺帧保持、有fault卸力 | 不复制对方允许旧关节而缺单颗年龄的部分 |
| 目标写包 | 地址42连续写6字节：目标+两组0，覆盖44/46 | 保留44目标电流，只写42目标，速度500只在不一致时写 | 对方写法与当前HD1910寄存器规则冲突，不能直接搬底层包 |

## 新增CPU对照

用我们已部署SHA003d1fa4…模型及replica几何，只改合成BAM增益/动作EMA，无hardwareAccess：

| 合成BAM增益 / EMA | 20秒最大倾角 | 末5秒最大关节峰峰幅 |
|---|---:|---:|
| 6 / 0 | 1.382° | 0.254° |
| 6 / 0.45 | 1.808° | 0.257° |
| 32 / 0.45 | 0.949° | 0.094° |

此前同条件32/无EMA最大倾角157.242°、末高度0.04737m，明显失稳。说明动作平滑在这一名义仿真能抑制高增益振荡；低增益6下平滑并未明显改善起始最大倾角，不能宣称越平滑越好或无需动力学匹配。此处32为BAM合成参数，不证明实物原始P32等同其32。未包含硬件D20/D40的准确建模，未做扰动场景/实物验收，不能直接上线。

## 优化优先级

1. 建立可回退、按模型明确选择的部署配置，把实际临时PID/动作EMA/速度来源写入诊断及模型运行记录。当前规则“不写PID”和“训练无EMA”保持，直到明确选定并验证新配置；不重写所有模式默认值。
2. 实物小幅阶跃识别响应；核对临时P/D寄存器生效及厂家固件语义，对照P6/D20候选与当前32/40。控制变量，不同时改PID、动作滤波、IMU标定。
3. 模型特定EMA=0.45候选先做扰动/观测延迟/初始姿态仿真，保持prev_action原始语义，再做托稳短闭环验收；需要匹配训练/部署参数，必要时重新训练。
4. 先验证速度寄存器50steps/s单位及实际速度一致性，再对照寄存器速度+低通与位置差分；继续保留真实接收时间与逐关节缺失判定。MCU100Hz与host50Hz解耦可参考，但我们50Hz已有实测能力，不因源码100Hz立即重构。
5. 新训练基于我们的实际质量、重心、足接触及HOME，扩大与实测误差匹配的噪声随机化；对方XgoDuck joint_pos噪声±0.01rad、gyro±0.06、gravity±0.03，大于我们继承配方±0.001/0.03/0.01，但应以测量确定范围。

这轮提出具体候选与边界，没有修改实机控制代码、PID、模型或标定。当前v5完整仿真验收历史未通过，不把此次几组名义CPU稳定结果冒称模型验收完成。

## 源码依据

- [xgoduck_rl/src/mjlab_microduck/robot/xgoduck_constants.py](https://github.com/LuwuDynamics/xgoduck_rl/blob/326d77a1122870bdefa2c36403937502c958e69c/src/mjlab_microduck/robot/xgoduck_constants.py#L100-L120)
- [xgoduck_runtime_arduino/sketch/duck_config.h](https://github.com/LuwuDynamics/xgoduck_runtime_arduino/blob/8cdbbd84710d856581982c9eaf0d5e2970666232/sketch/duck_config.h#L20-L34)
- [xgoduck_runtime_arduino/python/policy.py](https://github.com/LuwuDynamics/xgoduck_runtime_arduino/blob/8cdbbd84710d856581982c9eaf0d5e2970666232/python/policy.py#L100-L112)
- [xgoduck_runtime_arduino/python/rl_core.py](https://github.com/LuwuDynamics/xgoduck_runtime_arduino/blob/8cdbbd84710d856581982c9eaf0d5e2970666232/python/rl_core.py#L10-L65)
- [xgoduck_runtime_arduino/sketch/scs_bus.h](https://github.com/LuwuDynamics/xgoduck_runtime_arduino/blob/8cdbbd84710d856581982c9eaf0d5e2970666232/sketch/scs_bus.h#L111-L125)
- [xgoduck_runtime_arduino/sketch/sketch.ino](https://github.com/LuwuDynamics/xgoduck_runtime_arduino/blob/8cdbbd84710d856581982c9eaf0d5e2970666232/sketch/sketch.ino#L264-L306)
