# 2026-10-11 BLE发现故障

用户反馈：App搜索结果为空，无权限或错误提示。

板端检查：旧供体btd与统一服务同时广告，电脑实时回调曾看到同一MAC交替duck-ded7（6f5d2a10-3b47-4c8e-9a1f-2d7e8c4b6019）与DuckLink-f417aa（5d6b1000-6c69-4e6b-9f21-4475636b4c6b）。该初始回调未保存原始文件，仅作为现场观察记录，不冒充手机扫描日志或唯一根因证明。

- service-change.json：btd enabled/active→disabled/inactive；不改fake robotd。
- radio-after.json、radio-after-all-addresses.json：停旧广告后电脑暂时扫描为空。
- refresh.json：无活动动作后重启统一服务，BLE ready；15舵机使能/目标寄存器与标定保持。嘴部原保持使能未主动卸力。
- radio-refreshed.json：与App相同服务UUID过滤能搜到DuckLink-f417aa，RSSI −51。
- radio-stability.json：15秒无UUID过滤扫描回调6次新服务、0次旧服务，RSSI −50～−46。

最终只读控制器检查：powered=yes，ActiveInstances=1；btd disabled/inactive，microduck-observer active。控制器Alias仍duck-ded7属于系统适配器别名，不等于空中local_name；未改身份或配对。

没有实机运动、训练、模型更换或App更新。手机重新搜索/连接待用户确认；电脑恢复发现不代表手机连接验收。
