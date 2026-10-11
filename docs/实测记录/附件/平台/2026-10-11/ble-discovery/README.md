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

## 后台长连接后搜索失败

**2026-10-11 0.9.33再搜不到设备（残留LE已解除，代码修复待部署）：** 用户能收到几十台广播，App0.9.33无连接通知（Android13通知权限下不能仅凭无通知断言无前台服务）。主板统一Rust/BlueZ active、旧btd inactive、广告登记1实例，但一加Ace2 LE物理连接仍在；电脑10秒扫37台/无DuckLink，被动10秒ATT写/通知均0。新鲜只读确认disabled/15torque0后仅bluetoothctl断开该一加连接，未重启/动作/更改标定模型增益绑定；随后电脑36台中见DuckLink-f417aa及正确5d6b1000服务/RSSI−61，手机发现待反馈。源码发现通知结束仅drop_peer丢控制会话地址，没有Device.Disconnect，能留下物理连接；修正退出时断开对应peer，通知generation拒绝旧回调清理新订阅。未捕获原订阅结束事件，耦合恢复与源码遗漏不冒称唯一原因或一加已修复。122Rust/1忽略和ARM构建通过（新增2退出/旧回调回归）；初cargo默认examples既有缺module引用，采用生产bins检查，测试临时identity文件遗漏修正后全过。部署runtime174949在远端mkdir前因186 SSH超时终止，5722也关闭，未切服务，最后已核对仍runtime171053；已询问主板及手机搜索结果。无需新App OTA。证据附件/平台/2026-10-11/ble-discovery/long-connection-*.json、orphan-cleanup-audit.json及build/tests日志。

## 控制器重新初始化后手机确认

**2026-10-11 BLE搜索与连接已恢复（用户实机确认，最新覆盖）：** 用户明确没有断电，0.9.33仍搜不到；电脑复扫39台/无DuckLink，186和5722一度失联。只读本地设备ID扫描确认186恢复且地址未变，系统运行时间由先前10950秒变为约160秒、uptime启动时间17:53:10，期间确有系统重新启动，具体原因未确定，不归责用户断电，也未发系统关机/重启。空闲全15卸力下runtime175410完成部署，122Rust/1忽略、ARM构建、候选1000/1000完整反馈/20快照，模型/物理标定/增益/BLE身份/网页保持，旧runtime171053可回滚；通知结束断开物理peer和旧订阅generation保护已生效。部署及随后仅服务重新登记后仍无空中广播，HCI Extended Advertising Enable等命令Success、BlueZ实例1且无连接；不能把登记成功称无线成功。再次确认空闲15卸力后仅bluetoothctl控制器power off/on及重启统一服务，12秒接收4条正确服务广播（含DuckLink-f417aa，RSSI−58～−74）。用户随即确认App能搜到并连接，发现/连接已实机恢复；不改变绑定/标定，不发运动/训练，无新App OTA，继续0.9.33。controller复位加服务重登记恢复并不证明唯一固件根因或保证不复发，后台长期稳定/系统重启原因仍未验收。证据附件/平台/2026-10-11/ble-discovery/advertising-*.json、board-reboot-check.log、phone-recovery-confirmed.json及runtime175410/deployment.json。
