# HD1910 轮滑 30001：安装姿势与遥控

2026-10-25，在已合并的 `main / f805837` 上接入。仅完成本地代码和 ARM64 部署包；未部署、未发实机动作。

## 操作

| 阶段 / 按键 | 行为 |
|---|---|
| Back / Select | 全局卸力；进入轮滑前先确认卸力 |
| 双摇杆回中、LB＋RB 长按 1 秒 | 仅从卸力状态进入轮滑安装姿势，沿现有 1.5 秒 HOME 轨迹使能并到位 |
| 安装姿势 | 维持模型 HOME 固定目标，运行传感器/故障监视，不用 actor 输出驱动关节；玩家扶稳机器人安装四个被动轮 |
| 松开组合键，双摇杆回中后按 A | 从安装姿势进入零输入轮滑平衡；没有自动计时启动，也不经过坐站/步行模型 |
| 左摇杆上下 | 有符号推力，最大 ±0.4；小输入可能不足以起滑 |
| 左摇杆左右 | 转向意图±0.25；运行端按最大0.5rad/s更新目标航向，再计算夹紧±0.25rad的模型航向误差 |
| 松开摇杆 / 输入过期 250ms | 推力和转向意图归零，保留航向纠偏并继续模型平衡；转向回中/输入过期时锁定当前航向，被动轮可能继续惯性滑行，不保证主动刹停 |
| 再按 A | 归零输入，保持轮滑模式 |
| B | 停止模型并保持最后目标；再次使用前先 Back 卸力并重新进入预备 |
| Start 长按 | 保留原卸力后关机流程 |

轮滑模式固定 A 为平衡，不修改用户保存的普通手柄绑定。坐站、起身、翻滚、拾取、演示、自主等普通动作不在轮滑模式中执行。右摇杆头控与 LT 加速首版不启用；头部由 actor 控制。要恢复普通动作，先卸力并拆轮。重启、掉线重连不会自动启动模型。实体手柄只读调试模式仍不执行动作。

网页提供等价的“轮滑安装姿势”按钮，只有当前实机卸力时可用；轮滑模式的 A、摇杆、单位和禁用状态同步变化。网页不模拟双肩键组合手势。

## 模型与执行合同

- 权重：[swizzle_hd1910.onnx](../models/swizzle_hd1910.onnx)，[metadata](../models/swizzle_hd1910.metadata.json)。来源 `microduck-rl/models/candidates/swizzle_hd1910_30000/policy.onnx`，检查点 `model_30000.pt`，实际更新 30001 次。
- SHA256：`539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160`。
- 基于 HD1910 BAM、Observer 关节限位及四被动轮重新训练；任务 `Mjlab-Velocity-Swizzle-MicroDuck`。训练源码入口为 RL 仓库的 `src/mjlab_microduck/tasks/swizzle_observer_hd1910.py` 与 `scripts/server/train_swizzle_hd1910.py`。
- 61→14，归一化内嵌一次，50Hz native 同步循环；HOME＋raw action，scale=1，无动作 EMA、无电压倍率；raw previous_action、关节速度一帧延迟、机械目标边界沿用对应训练合同。头/身体命令填零，第三个外部 twist 分量为转向意图，经IMU航向闭环转换成模型航向误差。
- 独立 `swizzle` 槽位包含 `swizzle_prepare` 与 `swizzle` 两种执行状态。旧注册表仅增补缺失槽位，保留已有选择、回退、版本号及设备配置；启动只预加载轮滑的两个状态，不加载普通动作。
- 启动预备先检查软件卸力状态，再由唯一 UART owner 核查 15 个舵机实时 torque=0。模型哈希/加载/预热在使能前完成。A 切换复用控制会话、传感器与目标，不重新使能或重复 HOME。
- 实机持久增益仍使用统一设置，不改 EEPROM/零点/方向/IMU 标定。原故障、电压、IMU/关节过期保护保持；轮滑推理适用原行走跌倒卸力保护，安装姿势不当作自行平衡。

## 验证与边界

源模型 15 输入×16 环境×10 秒名义仿真 240/240 未倒。后退结合转向时几乎只转不退，原零推力大转向过快；本次限幅是首次实机试用的控制设置，不是重新训练，也未证明闭环实物稳定。

本地新增测试覆盖：组合键长按/回中/取消/只读调试、轮滑 A 覆盖绑定及急停优先、预备不接收方向、会话/序号/反馈门控、普通技能隔离、注册表迁移、遥控单位及边界。数值对照使用合成传感器 fixture，Python ONNX Runtime 与 Rust native 的观测及动作最大误差均为 0；目标编码检查通过，包含一次髋关节目标裁剪。13 个模型执行状态可加载，`hardwareAccess=false`。这些不是实机验收。

304项Rust测试通过（1项原有忽略）、87项前端测试通过，前端及ARM64构建通过。修改文本已检查UTF-8无BOM、LF。最终测试、构建和包内哈希记录见 [本地验证摘要](validation/swizzle-20261025/summary.json)。无新增依赖。轮滑会话复用现有 50Hz 控制循环，未做板端 CPU/RSS/实际频率测量。

## 构建与安装包

```bash
# backend-rust，Linux/WSL
cargo test --locked --offline --bin microduck-observer
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --locked --offline --release --target aarch64-unknown-linux-gnu \
  --bin microduck-observer --bin microduck-tof -j2
# frontend
npm run build
npm test
# 仓库根目录；只生成安装文件，不访问鸭子
python tools/deploy-runtime.py --package-only --preserve-device-settings --swizzle-model
```

输出目录在 `deploy/staging/runtime-时间/`，含 `payload.tar.gz` 与 `install.py`。这是现有 SSH 离线部署流程的包，不是网页“系统更新”签名包；不要直接上传到网页更新页。本机未找到已有发布私钥，因此没有伪造或更换签名密钥。

后续安装时将这两个文件放到目标鸭子的同一临时目录，在机器人扶稳、卸力后执行 `sudo python3 install.py <临时目录绝对路径> <runtime-版本名>`。安装器复制当前 release、叠加新后端/网页和轮滑 ONNX/metadata；保留原模型和持久配置，先验证 ARM 推理/模型库与只读反馈，再切换服务，失败回退。不要直接覆盖 current。新旧相同文件名权重不一致会被原文件保持校验拒绝，不自动覆盖已有不同轮滑版本。

本轮没有执行安装器。服务重启、实体组合键、装轮姿势、地面前后滑行/转向、真实掉线和恢复仍需实机验证。
