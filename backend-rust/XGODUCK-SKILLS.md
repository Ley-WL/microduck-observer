## 2026-10-12 翻滚结束门最新覆盖

当前kissqy翻滚8fdaf12c模型/无EMA/动作倍率1/50Hz保持。供体原版1秒固定结束；本机实测中途倾约84度，改为同观测重力累计正向旋转至少330度、经历倒置后直立小于15度且gyro范数小于1.5rad/s稳定200ms、至少1秒才切零速行走。最多2有效秒未满足则卸力；传感器暂停或切换清判据历史，普通恢复直立不算完成。此为本机执行适配，manifest1秒保留源合同；没有接触检测、没有自动连滚，不能把此门当作实机成功保证。已部署runtime-20261012-223342，实机保护验收待用户完成。其余下方旧参数按日期追溯。

> 当前工具入口见 deploy/RUST.md 和 tools/README.md。下述历史版本专用导入/部署脚本已删除，历史动作合同不能作为当前发布命令。

# XgoDuck 多模型后端

> **2026-10-11 最新执行覆盖：** 已部署kissqy-mechanical-v1，全部15关节机械范围采用kissqy保存标定。模型先裁机械角度、再按本机零位/方向转4096编码，无跨圈/编码饱和；单圈编码或EEPROM有效边界不满足则拒绝。模型权重、raw历史、增益及其它设置保持。旧训练XML/v3限位合同属于训练历史，和当前机械发令 guard 分开核验；旧段落的“仅v3裁角度/原供体编码饱和”已被本条覆盖。103检查、ARM五技能及152帧目标公式核对通过，只读部署非起身实机成功，详见实测03/04及2026-10-11/kissqy-mechanical证据。

> **2026-10-10 坐下修正候选（未部署）：** 新 `sit-self-contact-bounded-target-v3` 元数据配合RL独立SitStandContact任务，训练/CPU/运行统一EMA+HOME后14关节目标范围，再编码/EEPROM饱和；后端拒绝缺项/不同范围，raw prev_action不裁。原供体及已部署旧权重不因代码新增被重写为新合同。候选自身碰撞/较浅坐姿/顶住误差惩罚已短测，1000轮初看训练启动；模型尚未循环验收，板上仍旧sitstand5000。电压/故障/新鲜度保护、共享标定及EEPROM不变，见03/04最新记录。

> **2026-10-10 实物坐下失败：** 新sitstand5000出现腿/头顶住持续下压，两膝目标饱和、负载满量程、整组欠压至3.8～4.5V并卸力；纯ONNX故障观测重现大输出。原包正常，卸力后电压恢复。仿真验收不构成实物稳定证明；当前sit未实机通过，保护未修改，见03最新诊断。

## 2026-10-10 主板部署最新状态

已经5722部署release `20261010-hd1910-skills-v1`，起身4750/坐站5000权重及新完成判据、网页等待窗口和行走跌倒停止生效。八模型available、三个ARM数值核对通过、在线模型/二进制/首页哈希与本地一致；物理标定/舵机寄存器/BLE身份保持，蓝牙ready、IMU streaming。未发运动，实物动作验收待完成；ToF仍离线。下方同日“主板离线”及原版接入说明是历史。详细证据见实测03/04最新部署记录。

## 2026-10-10 本机重新训练模型替换（本地完成，主板离线）

`xgoduck_getup.onnx` 和 `alpha_sitstand.onnx` 保留旧接口文件名，当前内容已替换为本机 replica STL/XML + HD1910 训练的4750起身、5000坐站模型，元数据来源为hd1910_getup/hd1910_sitstand，不再称原版XgoDuck或官方alpha权重。起身完整GPU扰动最低99.5%/CPU100%，坐站97%/96.5%；完整训练证据在平级microduck-rl/qualified-models包中。其余行走/拾取/翻滚权重不变。

保留61→14、烘焙归一化、HOME增量、EMA0.45/0.55、raw prev_action与sit=1/stand=0标志。新站起至少4有效秒，新起身至少3有效秒，均要求倾角<15°、gyro范数<2rad/s、腿关节平均HOME偏差<0.25rad累计1有效秒后回零速行走，避免旧站起1秒提前截断。坐姿持续模型维持，4有效秒后开放站起；状态返回sitSettleSeconds，网页按该值禁用早期站起。这里是平台完成判据，非高度测量或实物到位证明。共享标定、临时增益、编码/EEPROM饱和与既有故障/IMU保护保持；原供体元数据兼容回滚。

导入`tools/import-hd1910-skills.py（已删除的历史脚本）`校验完整训练验收和哈希，旧本地权重备份到忽略的staging。部署使用`tools/deploy-hd1910-skills.py（已删除的历史脚本） --host <IP> --port <SSH端口>`；installer保留旧release，仅替换两模型、服务与网页，先隔离模拟/三动作数值核对，拒绝活动模型下切换，核对标定/寄存器/蓝牙身份，不发动作，异常回滚。当前主板5722连接关闭、186超时，未安装或实物验证。下方原版/官方说明属于历史接入。

后端沿用本机共享 ID、方向、编码零点、IMU 安装/参考和 HOME；不导入供体标定。所有模型为原版 ONNX，移植效果尚未实物验收。

| kind | 模型 | 启动与结束 |
|---|---|---|
| xgoduck | xgoduck_walk.onnx | 本机 HOME 后零速度平衡，接受按住方向指令 |
| xgoduck_getup | xgoduck_getup.onnx | 对齐当前位置后使能，不执行直立 HOME；倾角低于 15° 持续约一秒有效观测后切回平衡 |
| xgoduck_pick | xgoduck_pick.onnx | 直立启动；约四秒有效动作进度，前 40% 嘴部 30°、之后 0°，结束回到平衡 |
| xgoduck_roulade | xgoduck_roulade.onnx | 直立启动；约 1.9 秒有效动作进度，动作 EMA 旧 0.15/新 0.85，结束回到平衡 |

拾取内部模型指令为 `[cos(2πφ),sin(2πφ),0]`，是原版动作相位编码，不是外部行走速度。进度按成功接收的 50Hz 推理动作推进；反馈保持时冻结，实际墙钟时间可能更长。动作结束时仍倾倒超过 55° 则进入起身；不为普通 walk 自动开启倒地恢复。

## 接口

`GET /api/v1/policy` 列出模型及 `available`、SHA256 和元数据。`control` 中的 `requestedKind` 是启动类型，`activeSkill` 是实际当前模型，`skillProgressSeconds` 是动作进度；所有供体模型运行时 `kind=xgoduck` 表示同一控制家族。原有 v5/v6 不受影响。

独立启动使用 `POST /api/v1/policy/start`：

```json
{"kind":"xgoduck_getup","speed":0,"revision":当前标定版本}
```

`revision` 从 `GET /api/v1/calibration` 获取，需填实际整数。返回 `driveSession`；等待 `state=policy`。起身禁止 `/policy/home`，避免从倒地姿态先强行直立。拾取和翻滚可用对应 kind 独立启动，仍走本机 HOME。

在已经运行的 XgoDuck 平衡会话里，使用 `POST /api/v1/policy/skill`：

```json
{"session":"启动返回的driveSession","sequence":下一递增整数,"skill":"pick"}
```

`skill` 支持 `getup`、`pick`、`roulade`。动作与方向指令共用会话和严格递增序号；会话错误、重复序号、当前动作忙或反馈保持返回 409。同一时间仅一个动作，无动作队列堆积。动作过程中方向指令返回 409，切回平衡时清零方向；前端需要重新按住才能移动。`GET /policy` 为状态来源。

`POST /policy/shadow` 支持同样的 kind，沿用只读推理，不使能或发位置目标。`POST /policy/stop` 结束模型保持最后目标；`POST /servos/disable` 全部卸力。服务启动与部署均不自动运行模型。

## 执行约束

四个模型在使能前加载并预热；切换时清推理单槽、递增 generation、清原始上一动作和 EMA，避免旧模型异步结果下发。UART 始终由现有 owner 持有；嘴部按本机 0–30° 范围/共享零点换算，临时手动 P32/I0/D40，其余模型关节 P6/I0/D20。使能前先对齐位置，目标必须位于使能写之后；不写 EEPROM。原故障补读、通信保持、电压和硬件限位保护保留。

推理目标 50Hz、总线写读目标 100Hz 仍沿用现有部署；重复发旧目标也计入发令频率，不宣称实际频率达标。

部署入口：`python tools/deploy-xgoduck-skills.py（已删除的历史脚本）`。脚本先确认无运行任务，保存标定/寄存器证据，在模拟候选与 ARM 数值核对通过后切换独立 release，失败回滚；前端和已有模型保持。部署失败或主板不可达时不得宣称实机移植验收。


网页及DuckLink0.9.0已接入动作按钮（仅本地构建，未部署）。POST `/api/v1/policy/mouth` 接受 `{session,sequence,angleDeg}`，角度0–30°。只在walk平衡及有效反馈状态接受，与行走/动作共用递增序号；先归零行走，通过唯一owner对齐当前嘴部位置、临时P32/I0/D40使能，再写目标。拾取等动作优先，切换清待发嘴部目标并闭嘴。停机手动张闭嘴用既有servo angle接口。


## 2026-10-08 坐下/站起模型来源核查

用户要求接入sitstand。核查当前LuwuDynamics/xgoduck_runtime_arduino HEAD 8cdbbd84710d856581982c9eaf0d5e2970666232，只有walk/getup/pick/roulade四个ONNX；xgoduck_rl HEAD 326d77a1122870bdefa2c36403937502c958e69c有sitstand训练任务但没有导出ONNX，两仓库均无GitHub Releases附件。本地文件与训练产物未找到alpha_sitstand模型。前端该名字只是历史演示项，不是已取得的模型。需要供体ONNX/训练权重和对应观测命令定义后才能接入，未新增伪模型入口、未训练或部署替代固定姿势。来源检查证据docs/实测记录/附件/平台/2026-10-08/xgoduck-sitstand/source-check.json。


## 官方坐下/站起（2026-10-08已部署）

XgoDuck公开仓库无sitstand导出，但MicroDuck官方Hugging Face提供alpha_sitstand.onnx新版2250轮。已单独接入sitstand_sit/sitstand_stand（同一ONNX），使用现有会话POST /policy/skill的sit/standup。坐姿持续维持且不能走，坐下2有效秒后允许站起，站起1有效秒回零速度XgoDuck；这沿官方窗口，不是实际姿态到位证明。倒地起身仍getup。独立standup启动从当前姿态使能，禁止HOME。源版本/哈希/定义见models/alpha_sitstand.metadata.json。保留本机范围/标定/增益/保护，只读部署与数值核对通过，运动待用户验收。部署tools/deploy-official-sitstand.py（已删除的历史脚本），网页刷新，App0.9.6更多操作。


2026-10-10 当前坐站权重为用户选择的监督初始化单actor（SHA256 `2a3edcb880e88435a7ac248568859bb4f1eb487b9204db06a4aa3fb59c5ebe29`），不是原版模型或后续失败PPO。沿用hd1910_sitstand/v3合同：EMA+HOME后裁14关节目标、raw prev_action保持；坐姿持续推理，站起≥4有效秒且直立累计≥1秒后回零速walk。CPU重复循环31/32仅小样本初筛，hardwareValidated=false。release20261010-bidirectional-seed-v1已部署，实物动作待验收。

## 2026-10-10 坐站历史与逐帧诊断（已部署）

release20261010-sit-trace-v1保留同权重SHA及同HOME的sitstand_sit↔sitstand_stand原始动作/EMA；跨模型切换仍清历史和异步generation。每技能最多600推理帧、会话2000事件环形上限，保存精确obs/raw action/前后EMA/裁前裁后目标/编码及反馈；停止和故障写`/var/lib/microduck-observer/policy-session-latest.json`。故障先卸力再写文件，诊断写入失败只记warning。

用户新实测的精确ONNX重放及EMA核对通过，但原始输出已有多次反转，历史保留不是已证明的抖动修复。坐初期2秒原始膝目标累计行程596.51/492.52°，发令354.82/314.65°；实物可完成一次坐站但过程不平顺。权重仍2a3edcb880…，P/D/标定/EEPROM/保护保持。证据`docs/实测记录/附件/平台/2026-10-10/sit-trace/`。

## 2026-10-10 平滑actor候选（已部署，实物改善待验证）

当前alpha_sitstand是短程监督修正的单actor，SHA85d7e1a48f97d29af2482ad92ecf0685837e520d01960f34642feed0c82fc167；release20261010-sit-smooth-v1。只修改MLP权重，原图归一化/运行端EMA/HOME/目标裁剪/动作历史保持。以同指令新鲜相邻实机输入的输出差做平滑成本，完整仿真轨迹及全部实机输出做行为保留锚；300步，非PPO续训。新旧144例配对均134成功，无新增失败，但未正式验收。真实观测重放是拟合内结果，不能代表实物新闭环改善。

部署保留Rust trace二进制、所有其他模型、网页、标定、关键舵机寄存器、BLE身份和持久增益。旧sit-trace-v1完整保留回退。部署脚本tools/deploy-sit-smooth.py（已删除的历史脚本）先检查三组配对无新增失败，再复用活动任务拒绝、ARM数值核对、寄存器/标定核对及失败回退流程；不自动运动。硬件验收仍待用户新一次App坐站。

## 2026-10-11 kissqy同步控制核查

核对基准固定为kissqy提交94328f5b43c8ac825c90f0e564e7d1b926cd770d及各模型deployment-contract。行走/起身/坐站/拾取/翻滚均同上游权重；兼容文件名xgoduck不代表仍用旧XgoDuck模型。所有kissqy入口现在同步50Hz read→observe→infer→write；旧本机训练模型入口仍独立保留，不把它们称作kissqy模型。

起身/坐站从当前位置使用模型增益上力；HOME用模型增益和两秒线性过渡。起身直接启动的六秒窗口结束回HOME保持；运行中接管起身则到期交回行走。没有达到直立不会无限重复同一模型，也不会将窗口结束当成成功。原始动作历史独立于电压倍率，native速度延迟随采样推进一次。电压补偿采用固定提交源码默认true，而非已读到供体现机开关；此选项及用户持久增益是运行配置，不能靠模型哈希宣称两台实机设置一样。

本机设备适配仍保留已校准外置IMU、编码零点/方向、EEPROM、工作电压/故障/数据年龄和用户要求的行走跌倒卸力；供体的全套robotd外设、音频/游戏手柄/低电量关机等不属于本次策略移植。新发布只读验收，不是实机起身成功验收。完整证据见docs/实测记录/附件/平台/2026-10-11/kissqy-native。
