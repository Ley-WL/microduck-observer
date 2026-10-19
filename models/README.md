> **2026-10-19 用户反馈v62b效果不如前版，已明确选择并回退原版抬左脚：** 224当前为runtime-20261019-flamingo-official，原版权重df77929c…及配套原二进制SHA63c086ec…已核对；两金鸡入口official_flamingo、全部模型可用，20次只读快照15颗卸力。关节标定、模型增益及手柄debug模式保持；未触发动作。v62b发布包保留，切换前最近会话/失败记录已归档。此次为恢复此前行为，不代表原有抖动已修复或实物验收通过。本地v62b代码和模型仍保留，之后部署勿误选。证据：docs/实测记录/附件/平台/2026-10-19/rollback-official/rollback.json。

> 2026-10-19：用户指定v62b已部署至224，活动权重/配置为 `rear_hook_v62b.onnx` / `rear_hook_v62b.metadata.json`，runtimeKind=`hd1910_rear_hook_v62b`。原始 `candidates/rear_hook_v62b` 包保持不变，其中未部署说明为发布时历史；本次配套时钟已实现并验证。仍未实物验收，原版及旧后抬文件保留回退。

# Observer 模型

## 金鸡独立模型的区别

本次自行训练的模型在 [candidates/rear_hook_v62b/](candidates/rear_hook_v62b/)，采用 **复刻项目 STL/MJCF + HD1910（1910 M6）执行器参数**，使用 v53 第 500 轮权重与 v62b 等待时钟。它是候选归档，开发集完整周期 242/256（94.53%），未通过验收，Observer 已支持其时钟合同；当前活动入口优先使用原版 flamingo_official，v62b 保留为候选及回退记录，不代表实物验收通过。

Hugging Face 的 [RemiFabre/microduck-flamingo-cycle](https://huggingface.co/RemiFabre/microduck-flamingo-cycle) 是另一个上游发布模型，采用 XL330 配置。下载该模型或克隆上游 RL 仓库，不会获得本地训练的 HD1910 v62b 权重。

具体模型身份、权重 SHA256、执行合同和验证边界以各模型的 metadata 及说明为准。
