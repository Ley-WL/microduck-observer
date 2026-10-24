# 网页服务更新

2026-10-23：已通过首次引导部署到 192.168.137.135，版本 runtime-20261023-161122；接口就绪、真实无效上传拒绝、资源和硬件只读验证通过。正常签名包通过网页安装及断电恢复仍未实测。

## 操作流程

侧栏「系统更新」独立于姿态界面。选择项目工具生成的 `.tar.gz`，上传后查看已验签的版本、组件和文件数，再点击安装。支持前端、后端、模型三个组件的任意组合；组件由更新包决定。普通源码 ZIP、手工压缩目录不能直接安装。

安装前必须停止控制并让全部 15 个舵机在线、卸力，遥测新鲜。安装过程中锁住新的控制和配置写入，保留卸力入口。服务重启会短暂断开页面，页面自动轮询，成功后刷新加载新网页。

## 生成更新包

先构建前端和 ARM64 后端，再在仓库根目录运行（Python 需要 cryptography）：

```sh
python tools/package-web-update.py --version 2026.10.23.1 --output dist/observer-2026.10.23.1.tar.gz
python tools/package-web-update.py --components frontend --version 2026.10.23.2 --output dist/frontend-2026.10.23.2.tar.gz
python tools/package-web-update.py --components backend --version 2026.10.23.3 --output dist/backend-2026.10.23.3.tar.gz
```

默认使用 `frontend/dist` 和 `backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer`；可通过 `--frontend`、`--binary` 指定产物。版本名称只接受字母、数字、点、下划线及短横线。`--code` 默认当前 UTC 秒数，必须高于设备上一次网页更新包；同一批发布应使用同一个 code。

签名私钥默认 `~/.microduck/release-manifest-key.pem`，也可用 `--key` 指定，必须匹配设备安装的 OTA 公钥。私钥只在发布电脑使用，不放进压缩包或仓库。上传不需要访问原有 OTA 下载服务器。

## 旧版本首次接入

没有 `/api/v1/update` 的老版本无法自行增加上传入口。需要通过现有部署方式一次性安装本版前后端，以及本版 root 更新助手、公钥、socket 和恢复单元。

构建 ARM64 helper 后可运行以下命令准备引导文件，不连接设备：

```sh
python tools/provision-observer-ota.py --package-only
```

输出到 `deploy/staging/observer-ota-bootstrap`。完整引导部署须安装其中的 helper、公钥、四个 systemd 单元及 `update-recovery.conf`，执行 daemon-reload 并启用 OTA socket；现有脚本提供远程安装入口。实际部署必须明确目标主机、端口和凭据文件，不能沿用脚本历史默认设备地址。公钥已存在时要求一致。

安装新前后端并重启后，检查 `GET /api/v1/update` 返回 `available: true`、`protocol: 1`。之后的兼容服务版本可从网页更新。不同系统目录、CPU 架构、驱动或运行库合同的设备，仍需专门迁移，不能据此宣称支持任意历史版本。

## 包格式与安装边界

- gzip + USTAR，第一个文件为带 RSA/SHA256 签名的 `manifest.json`；其余为 `payload/` 下的普通文件。
- 清单 schema 为 `microduck-observer-bundle/v1`，architecture 为 `aarch64`，updaterProtocol 为 1；逐文件记录大小、SHA256。
- 压缩包及解压文件总量各不超过 128 MiB，单文件不超过 100 MiB，最多 2048 文件。拒绝路径穿越、符号链接、重复文件、清单外文件、损坏包及错误架构。
- 允许 `bin/microduck-observer`、可选 `bin/microduck-tof`、`licenses/`、`frontend/dist/`；模型组件另外允许 `models/` 顶层成对的 `.onnx` 和 `.metadata.json`；包不能包含安装脚本、标定、身份文件、系统服务或驱动。
- 保留现有 release 的运行库及未被包覆盖的模型和其他资源，复制为新目录后覆盖所选组件。切换前执行模型库校验；前后端单独发布需要发布者确认接口兼容。
- 根权限助手只负责固定更新步骤；被上传的服务按 radxa 身份验证和运行。后续更新 root helper 本身仍需单独部署。

## 失败处理

新目录准备好后才停止旧服务并原子切换 current。启动检查包含 15 舵机卸力遥测、实际运行程序路径、更新接口、网页文件 HTTP 哈希和既有配置内容。失败执行回退，独立 systemd 任务与 root 日志用于进程中断及开机恢复。恢复未完成时保持控制锁定。

原 release 保留，更新前检查复制空间；不会自动清理历史 release。配置备份覆盖已有状态 JSON 和 DuckLink 文件，不支持任意版本的数据迁移脚本。切勿把这些恢复机制描述为已通过实物断电测试。

## 验证

本地 Rust 主服务 292 通过、1 忽略；helper 8 通过；前端 86 通过及构建通过；ARM64 release 构建通过。`python3 tools/test-web-update.py` 使用临时签名密钥和本地 helper，覆盖三种组件包、篡改、错签、缺失/重复、路径穿越/链接/架构、截断/追加归档、超限等测试。

尚未验证：真实设备上传安装、重启后网页交互、安装失败回退、进程强杀及断电恢复。上述本地结果不代表整机或运动验收。

## 同包更新模型（2026-10-23）

```sh
python tools/package-web-update.py --components backend frontend models --models models --version 2026.10.23.4 --output dist/full-update.tar.gz
python tools/package-web-update.py --components models --models release-models --version 2026.10.23.5 --output dist/models-update.tar.gz
```

`--models` 指定目录的顶层 ONNX 会连同各自同名 `.metadata.json` 一起打包；只更新部分模型时，用只包含这些文件的目录。没有选择 models 组件时不会打包模型。模型和 metadata 必须成对，仍受签名、哈希、128 MiB 总量限制。

按相对路径覆盖或新增，未包含的模型保留。预览列出模型文件名。新旧模型位于各自 release 中，失败回退时一并恢复旧版；不会直接改写当前运行目录。

保留设备上的模型选择、自定义动作注册表和通过模型管理导入的文件。更新同名内置模型后，使用该文件的动作会加载新模型；全新文件需要通过模型管理注册/选择，不会自动绑定动作或手柄。`models/registry.json` 不打包（当前默认注册表编译进后端），不会用包内注册表覆盖设备设置。

切换前继续执行现有模型库加载校验，覆盖运行器选择的模型；此项不等于所有未选模型均完成推理验证，更不代表动作实物验收。新增模型的完整兼容性仍由模型管理加载验证。此扩展尚未进行板端更新验收。

本轮本地验证：helper 8 项测试通过，更新包集成测试扩至 9 组并全部通过（含模型单独包/三组件包、缺少配对文件和禁止覆盖注册表等），ARM64 helper 和前端构建通过。
