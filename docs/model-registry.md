## 自定义动作（2026-10-22）

模型管理点击「＋ 新增动作」，填写动作名称/ID、模型名称、ONNX及配置来源。新动作使用独立的`custom-` ID，不替换原有动作。当前可选择舞蹈、拾取、翻滚、左右踢球的兼容执行模板；其观测、关节顺序、命令及结束条件必须与训练一致，不支持任意网络或未适配任务。官方manifest仍按已有适配范围校验。

创建与首个模型验证为同一事务，失败不留下空动作。成功后手柄设置下拉框自动提供该动作（已打开页面可点重新读取），保存后网页和实体手柄共用映射，重启保留。先站立并回中后触发，按下仅执行一次；急停与停止保持不受影响。动作及权重在运动启动前预加载，切换不在串口循环加载文件。最多16个自定义动作；后续可继续导入候选、应用或回滚，无删除/任意重命名接口。

新增动作默认选用首个验证通过的模型，但不会启动运动。新配置只能在控制停止后保存；实机动作仍需另行验收。

# 模型注册与选择

2026-10-21：本地前后端实现，未部署或验证实物动作。

侧栏独立页面“模型管理”支持按动作选择权重、导入配套 ONNX/metadata、回到上一选择。模型导入仅注册候选；应用或回退只保存选择，下次启动模型生效，不会使能舵机或发运动命令。Web、手机和实体手柄随后使用同一组选择。

## 使用

1. 停止模型及其他控制任务，打开侧栏“模型管理”。
2. 替换已有模型：选定对应动作的候选，点击“应用”。服务先检查合同、SHA256及实际ONNX张量接口，再用生产推理入口预热。
3. 导入自训权重：选择动作、填写名称，上传 `.onnx`，配置来源选择默认执行模板、官方 manifest 或完整 metadata。ONNX 上限16MiB，JSON上限256KiB。通过后在该动作列表选择并应用；上传不会自动切换。
4. “上一版本”会重新校验此前的选择，成功后切回。若文件已损坏或缺失则拒绝，不会静默选其他文件。

坐下/站起共用 `sitstand` 槽位；抬脚/落脚共用 `flamingo` 槽位。同一动作的进入和退出不会选到不同权重。界面显示文件校验失败原因、执行类型、SHA摘要和metadata中的验收状态；“可用”只表示文件与metadata校验通过，切换时还须通过ONNX加载与预热，不代表实机动作合格。

## 注册表与执行合同

- `models/registry.json` 是编译时内置初始注册表，明确指定九个槽位及候选文件。当前金鸡默认右脚3000模型，与本地现役配置一致。老设备缺少该文件时明确显示不可用，需要选择已安装候选，不按文件存在顺序自动回退。
- 首次修改后，完整注册表持久化到共享标定目录旁的 `model-registry.json`。可用 `MICRODUCK_MODEL_REGISTRY_FILE` 指定隔离位置。重启读取保存的选择；发布包不得覆盖此文件。
- 用户上传的权重与metadata保存到该目录旁的 `model-artifacts/<生成的ID>/`，不写入只读release。注册失败时清理本次文件，当前配置不变。普通更换候选不会覆盖旧文件。
- `policySha256` 仍必须与实际ONNX字节完全一致。右脚金鸡、官方金鸡和v62b不再在Rust中绑定具体SHA；`runtimeKind`、任务、命令侧、EMA、时钟和限位合同继续按已有实现验证。含 `walkExecution` / `skillExecution` 等嵌套哈希的metadata仍须同步更新。
- 本次支持“已有执行合同的新权重”，不是任意新技能或任意网络。仍为单输入 `[1,61]`、单输出 `[1,14]` 的float32前馈模型；新命令编码、外部时钟或LSTM仍需增加对应运行实现。
- HOME、关节顺序、原始动作历史、目标处理、硬件标定和现有保护保持原模型合同；全局模型增益仍由 `model-gains.json` 控制。

## 接口

| 方法 / 路径 | 请求 | 结果 |
| --- | --- | --- |
| GET `/api/v1/model-registry` | 无 | `schemaVersion`、`revision`、`models`、`selected`、`previous`；每个候选附 `available`、metadata或错误 |
| POST `/api/v1/model-registry/select` | `revision`, `slot`, `id` | 校验、预热后保存选择 |
| POST `/api/v1/model-registry/rollback` | `revision`, `slot` | 校验并切回上一选择 |
| POST `/api/v1/model-registry/import` | `revision`, `slot`, `label`, `onnxBase64`, `metadata` | 注册候选，不改变选择 |

写请求沿用JSON与Origin检查；携带旧revision、槽位不匹配、模型无效或已有控制任务均拒绝。服务持有与HTTP/BLE控制相同的运动任务锁，直至校验和保存完成；浏览器断开不会提前释放锁。文件保存成功后才更新内存选择。配对动作原子地引用同一槽位。最大注册数128；暂不提供删除入口，上一版本按钮只记录每槽位的上一个选择。

导入和预热在阻塞任务中执行，不放入50Hz控制循环。最大16MiB权重经Base64和JSON上传会产生数份临时内存副本；这是停止控制后的管理操作，不能当作实时推理内存或时延。实际板端上传峰值与耗时尚未测量。生产状态目录需保留写权限和足够磁盘空间。

## 本地验证

```sh
cd backend-rust
cargo test --locked --offline --bin microduck-observer
cargo build --locked --offline --bin microduck-observer
cd ..
MODEL_REGISTRY_TEST_BINARY="$PWD/backend-rust/target/debug/microduck-observer" \
ORT_DYLIB_PATH=/absolute/path/to/libonnxruntime.so \
python3 tests/test_model_registry_api.py
cd frontend
npm test
npm run build
```

集成测试使用临时状态目录和随机本机端口，明确设置模拟模式、关闭BLE、串口及ToF。覆盖真实ONNX上传预热、导入不激活、旧revision拒绝、错误槽位、选择/回退、重启保持、错误SHA、正确SHA但损坏图的拒绝及失败后继续使用。没有提供测试二进制和匹配运行库时明确跳过，不能当作集成测试通过。

本次本地验证：Rust 270项通过、1项外部fixture忽略；前端82项通过，Rust本机二进制和网页生产构建通过。ONNX Runtime 1.23.2下两组API集成测试通过，覆盖默认模板、官方踢球集manifest、Flamingo单模型manifest、导入不激活、错误配置拒绝、持久化和回退。15套内置模板均验证保持原执行合同，且不复制训练来源与验收结论。浏览器验证独立页面、默认模板和官方manifest配置切换；姿态页无模型管理表单。未运行ARM构建、板端上传性能或实物动作测试。

部署前需将新二进制和网页一起发布，并确认目标机器所有选中权重实际存在；候选验证使用复制的注册表与上传模型目录，避免修改生产选择。现有固定权重fixture必须与所选候选匹配，不可拿旧权重期望输出验证新候选。回滚到不支持注册表的旧二进制后，其旧文件选择行为会恢复；跨二进制回滚应连同模型路径和选择一起核对。

## 独立页面与默认配置

侧栏「模型管理」为独立页面；默认首页仍为「姿态与 IMU」，姿态工作台不再嵌入管理表单。

导入支持三种互斥来源：
- 默认执行模板：选择动作和内置模板，上传 ONNX 即可。后端复制执行配置、计算新 SHA256，更新嵌套执行合同 SHA；不继承旧模型训练来源、仿真或实机验收结论。模板仅适用于同观测/动作语义和时序的模型，形状预热不能证明训练语义一致。
- 完整 metadata：保留已有严格校验和原始 SHA 比对。
- 官方 manifest：schema 2，单模型或 `policies[]` 模型集；模型集按上传 ONNX 原始文件名精确匹配，重复或缺失条目拒绝。当前支持 BallKick 左右脚（episodic / 0.5 秒），以及 FlamingoCycleHard（perpetual / 1.5 秒落脚，当前执行器左腿抬起）。官方集的标准 `ball_kick_left/right.onnx` + `kick_left/right` 条目可省略 task_id；其他单模型需提供 training.task_id。

官方 manifest 与 Observer metadata 不是同一套 JSON。适配后仍使用 Observer 的执行器及已有保护，不代表复刻官方 robotd。仅支持 API 1 的 61→14 前馈 ONNX、scale 1、50 Hz；LSTM/API 2、其他任务、phase/posture_flag、自定义 unwind/ramp 尚未接入此适配器。原 XL330 模型在 HD1910 上不自动获得实机验收。

API：GET 增加 `presets[]`（id/slot/label/metadata）。POST import 在 `metadata`、`presetId`、`manifest` 中三选一；manifest 模式同时传 `onnxFilename`。所有方式仍经过 SHA、合同、ONNX 预热和原子持久化，导入不会自动选中。
