# 当前拾取：HD1910 官方配方 1001 更新

[模型](xgoduck_pick.onnx) · [metadata](xgoduck_pick.metadata.json) · [训练配方与测试](https://github.com/Ley-WL/microduck-rl/blob/main/models/pick_hd1910_1001/README.md)

2026-10-22，233已选用 seed221149 的1001更新模型，用户反馈动作流畅并确认提交。该文件更新为当前自训权重；`kissqy_ground_pick` 是兼容运行合同标识，不再表示权重来自 KissQy。

SHA256：`b9f88799be0f979115f8d22fefbc3d4db8c95e4567f6ac04f42c6203cece5141`。50Hz、61维观测、14维动作、内嵌归一化、HOME+raw动作/无EMA、4秒相位、2.8秒交回行走。沿用板端增益设置及覆盖规则。

32例仿真下探中位数约29mm、回站窗口稳定，无重置；尚未通过本地25mm三周期判据或全面物体拾取验收。原模型可从历史提交 c842350 的同路径恢复；233板端注册表保留旧 pick-default 作为上一选择。

坐站稳定后提前切行走属于独立服务修改，见 [切换条件和验证](../docs/sitstand-stable-handoff-20261022.md)。模型来源及许可沿用RL仓库声明。
