# XgoDuck 多模型后端

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

部署入口：`python tools/deploy-xgoduck-skills.py`。脚本先确认无运行任务，保存标定/寄存器证据，在模拟候选与 ARM 数值核对通过后切换独立 release，失败回滚；前端和已有模型保持。部署失败或主板不可达时不得宣称实机移植验收。


网页及DuckLink0.9.0已接入动作按钮（仅本地构建，未部署）。POST `/api/v1/policy/mouth` 接受 `{session,sequence,angleDeg}`，角度0–30°。只在walk平衡及有效反馈状态接受，与行走/动作共用递增序号；先归零行走，通过唯一owner对齐当前嘴部位置、临时P32/I0/D40使能，再写目标。拾取等动作优先，切换清待发嘴部目标并闭嘴。停机手动张闭嘴用既有servo angle接口。


## 2026-10-08 坐下/站起模型来源核查

用户要求接入sitstand。核查当前LuwuDynamics/xgoduck_runtime_arduino HEAD 8cdbbd84710d856581982c9eaf0d5e2970666232，只有walk/getup/pick/roulade四个ONNX；xgoduck_rl HEAD 326d77a1122870bdefa2c36403937502c958e69c有sitstand训练任务但没有导出ONNX，两仓库均无GitHub Releases附件。本地文件与训练产物未找到alpha_sitstand模型。前端该名字只是历史演示项，不是已取得的模型。需要供体ONNX/训练权重和对应观测命令定义后才能接入，未新增伪模型入口、未训练或部署替代固定姿势。来源检查证据docs/实测记录/附件/平台/2026-10-08/xgoduck-sitstand/source-check.json。


## 官方坐下/站起（2026-10-08已部署）

XgoDuck公开仓库无sitstand导出，但MicroDuck官方Hugging Face提供alpha_sitstand.onnx新版2250轮。已单独接入sitstand_sit/sitstand_stand（同一ONNX），使用现有会话POST /policy/skill的sit/standup。坐姿持续维持且不能走，坐下2有效秒后允许站起，站起1有效秒回零速度XgoDuck；这沿官方窗口，不是实际姿态到位证明。倒地起身仍getup。独立standup启动从当前姿态使能，禁止HOME。源版本/哈希/定义见models/alpha_sitstand.metadata.json。保留本机范围/标定/增益/保护，只读部署与数值核对通过，运动待用户验收。部署tools/deploy-official-sitstand.py，网页刷新，App0.9.6更多操作。
