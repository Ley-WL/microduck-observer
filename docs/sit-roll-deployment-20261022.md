# 坐站3000 / 翻滚1000 实机试用发布

用户2026-10-22指定192.168.137.23。新模型：坐站9efda065（仿真64/64）、翻滚4d2b2a72（仿真32/32）；尚无本次实机动作验收。

发布基础是当前已部署f51f5f9源版本，独立构建于.local/sit-roll-release，不混入工作区正在开发的模型注册表/手柄界面。现役包runtime-20261021-flamingo-right-3000保留整包回退。只更新服务程序、alpha_sitstand与xgoduck_roulade模型和metadata；其它模型、网页、标定及持久设置保持。

新runtimeKind分别hd1910_official_sitstand和hd1910_official_roulade；61→14、50Hz、HOME+raw、原始action历史、无EMA、无额外电压倍率、Observer有效限位。坐站站起窗口6秒且连续稳定至少0.5秒，坐姿保持等待用户站起；翻滚5秒完整窗口后仍需旋转及稳定落地门控，失败卸力，不能靠时间自动判为成功。原版执行合同保持。

262项后端测试通过、1项外部fixture忽略，ARM release构建通过。三组fixture使用新权重重新计算期望action。完整cargo test受到历史examples缺失模块影响，使用生产二进制测试目标；隔离副本编译所需deploy和model.json也取自同一Git版本。

拟发布runtime-20261022-sit3000-roll1000；部署成功以板端deployment.json为准，本文创建时程序仍在上传。未发运动指令。

## 部署完成

192.168.137.23已切换runtime-20261022-sit3000-roll1000，旧包保留回退。三项板端fixture观测误差0、action最大误差1.19e-7，编码/机械限位检查和模型bank通过；连续20次快照确认15舵机在线卸力。其它模型/网页/增益/手柄/标定/配置保持检查通过，未发动作。证据：[deployment.json](实测记录/附件/平台/2026-10-22/sit-roll/deployment.json)。
