> **2026-10-11 App拾取不张嘴修复（Rust已部署，仅只读验收）：** 当前kissqy拾取actor只有14维，不包含嘴#34；之前严格对齐取消自动开合，native初始化嘴−5且只消费独立intent，App pick没发嘴目标，旧pick-mouth核查未修复。现用户要求拾取能张嘴，Rust native增加本机组合：前1.6有效秒30°，后段闭−5°，2.8秒拾取合同不变；时序复用本机旧非native脚本，不称kissqy模型输出/供体默认，不证明对应实际接触时机。反馈暂停不推进phase；正常结束或切其它模型清pending_mouth并闭嘴；用户停止保持维持最后目标、急停卸力，不追加嘴运动。trace/control新增嘴目标/来源及帧progress便于跟随核查。身体模型/权重rawhistory/PD、嘴P10D20/限位、标定EEPROM/IMU/BLE均保持，不联动叫声。131Rust检查/1外部忽略、ARM构建通过；空闲15卸力下runtime210030已安装、旧204748可回滚，999/1000完整候选帧（一帧缺32/33）/20生产快照卸力，模型网页/增益身份保持、tofd/rsyslog masked。网页/App现有pick统一生效，无App改动或新OTA，仍0.9.35。未训练/仿真/代发动作，开合与拾取效果待用户验收。证据附件/平台/2026-10-11/pick-mouth-auto及runtime-20261011-210030。

> **2026-10-11 坐下预备/站起衔接及行走差异修复（已部署/App OTA，仅只读验收）：** 用户暂停训练，预备换坐下、App平衡换站起并核查后退。预备启动sitstand_sit，从实读当前位置对齐使能维持坐姿，不走直立HOME；站起按主板sitSettleSeconds（kissqy4秒，旧App2秒修正），sitstand_stand1有效秒同会话交回行走，无HOME/卸力插入。原倒地入口及起身失败卸力保护保持。供体94328f5行走cmd_alpha=.2此前缺失已补50Hz平滑（超时立即归零本机保护保留）；训练范围vx±.4/vy±.3/yaw±1，旧vy±.1/yaw±.5扩至供体；网页显示.4而wheel隐藏限.2已修。App下推负号/负速度完整链路回归通过，显示实际模型commandTwist；权重1cbf/倍率.9/头腿滤波.5/.7/rawhistory/61维/gyro/速度延迟/电压适配不变。明确保留差异：本机持久P5D0、供体管理默认P3D0（非实机回读），本机输出上限1000/供体默认681、安全交互未完全同，不擅自改用户增益。最新session只有start，不能对应后退失败，未证明根因或实机成功。130Rust/1外部忽略、网页59/构建、Android回归lint通过；空闲15卸力下runtime204748安装，999/999完整只读帧/20快照，模型/标定EEPROM/IMU参考/增益/BLE保持，tofd/rsyslog masked。App0.9.35/code54原签名OTA公网验签/APK逐字节长度SHA/下载页通过。没有训练、仿真或代发运动，坐站/后退/转向待用户验收。证据附件/平台/2026-10-11/sit-prepare-walk及runtime-20261011-204748、平级mobile/releases/sit-prepare-0.9.35。

> **2026-10-11 起身结束仍倒地→HOME使能问题已修复（已部署，只读验收）：** 用户反馈起身失败、倒地预备仍使能。最新session1791721535100/a481/getup300帧，初89.975°/末92.303°/事件94.7005°，单帧最低1.156°不能当稳定站起；原6秒窗口结束仍返回HOME且关节全部±5°到位，result holding/recoveryUprightAtEnd=false，确证关节HOME不能证明身体直立。更晚failure1791721612547是另一次home-transition #23−5.27°，不混作同会话。首次只读snapshot当时新walk/15torque1；用户称没使能后立即复读age4.45ms确认disabled/15torque0，最新回读覆盖前态，未代发停止。修复保留6秒模型窗口，但结束仍≥60°按现有倒地判据记录失败并停止推理卸力，禁止HOME；网页/App预备、模型初始HOME与起身结束HOME共用姿态预检，倒地时拒绝目标及使能写入，过渡中再倒亦中止卸力。这是平台对kissqy无条件到期回HOME的明确失败处理补充，不称供体自带；零位/单关节、模型权重/PD/rawhistory/标定EEPROM/BLE及IMU归零保持，未训练。129检查通过/1外部fixture忽略、ARM构建成功；空闲15卸力下runtime-20261011-203144已安装，前201917可回滚。候选999/999完整只读关节帧，20在线快照卸力，service active/二进制SHAa7e69bd4…，网页/全部模型旧新diff相同且设置hash保持，tofd/rsyslog仍masked。不发实机动作，修复不保证模型能起身，尚待用户运动验收。证据附件/平台/2026-10-11/getup-home-failed及runtime203144。

> **2026-10-11 kissqy起身切换条件修复（已部署，只读验收）：** 用户要求切换不一致直接改。固定94328f5 robotd LB/控制step核对：普通停机起身6秒→HOME、运行策略起身6秒→原策略及native HOME2秒原本一致；此前“固定会话入口就是错误去向”表述不成立，不能当低头根因。实际差异已修：直立策略LB保留raw action/滤波状态而非总清零；从坐姿进入且未倒地，完成后恢复原坐姿及已过settle进度；确认倒地>60°持续200ms才重置/抢占拾取翻滚，直立忙动作拒绝；重复LB确认但不重启、不推迟6秒窗口，HOME结束优先；停机未完成native HOME/卸力起身先只读确认倒地才上力。新增native_homed，仅完整native HOME到位置true、卸力状态清false、服务重启默认false，直立入口还要求实时torque-on。继续保留用户要求的停止保持和独立行走跌倒卸力保护，不声称整个安全/会话状态机完全复制供体。127检查通过/1外部推理忽略，ARM release完成；主板空闲15卸力下runtime-20261011-201917已安装，旧183231可回滚。候选只读1000帧999完整（一帧缺31/32），20快照卸力，native active、二进制SHA45138584…；原/新网页及全部模型逐文件diff相同，标定/增益/BLE身份hash保持，tofd/rsyslog仍masked。没有新模型、App OTA、训练或代发实机动作。上一300帧getup没有中途切换；此修复未证明低头问题解决，仍需用户动作验收。证据附件/平台/2026-10-11/getup-switch/{comparison,verification}.json、测试构建及部署runtime201917。

> **2026-10-11 对应倒地起身完整采集（最新，只读）：** 用户按要求完成一次倒地起身→停止，并确认结束后仍低头。新session SHA980db9e2…300帧全部xgoduck_getup/a481，颈/头P5D20、torque1/fault0，没有切行走，6有效秒后HOME；gravity正确取obs[3:6]归一化，起始94.839°、末模型9.434°、完成事件11.246°，约1.568秒采样9.075°但随后有回摆，不称持续稳定站直。末50帧颈目标−90～25.400°，相邻目标差p95 43.541°、写前反馈与上一已发目标误差中位20.962°；头pitch相应−9.141～90°、23.502°/14.502°。快速变化和跟随偏离均存在，不判单一脖子力弱或PD根因。返回HOME颈/头pitch均20.001956°，逐项与固定kissqy model-info的HOME一致（误差<1e−6°）；完成时编码误差3.25/−2.29°，按direction−1还原实际约16.752/22.292°，非大幅未到目标。此仅到位瞬间，之后68.36秒至卸力缺连续保持反馈，不能排除后续下垂/姿态期望差异，也不能把卸力后的snapshot当保持验收。用户称结束后仍低头，故动作末段跟随滞后不能完整解释。供体无颈部独立PD证据保持；不加P/D、不改模型/HOME/标定/EEPROM、无部署训练或代发运动。下载getup-failure SHA b73da15b…eventTime1791719270633早于新session1791719768927，是旧坐站/行走failure不适用于本次。**纠错：旧头颈分析用obs[2]（gyro）当gravity，旧90→77°数字撤回；重算sitstand_stand为8.981→32.068°，此前所推体态不成立，未据旧数改设备。** 同时修复分析文件名被关节名覆盖、短组上一帧对齐长度；缓存42寄存器不当逐帧目标回读。详见附件/平台/2026-10-11/getup-neck/getup-session-analysis.json及README。

> **2026-10-11 起身头颈抬不起/供体PD核查（只读，待对应起身采集）：** 用户说明倒地起身/爬起身体已起来而头前低，要求核对kissqy颈部独立PD。固定供体94328f5的hd1910.rs基础路径所有非mouth关节统一firmware_p/D20，本模型P5；只有mouth P10。另下载同提交duck-control/src/hd1910_test.rs SHAcd8d8519…，servo-test-parameters仅统一kp/kd/torque_limit、rows按全部snapshots应用，没有颈部单关节例外；管理默认3/0/681不是供体实机回读。Observer donor技能统一P5/D20、嘴P10/D20；行走独立持久配置板当前P5/D0（原文件只读，不改），切换回行走会换整组增益，不是脖子专属。当前下载完整session SHA08aaf1b3…及failure b73da15b…；该session950帧含walk686/sit210/stand50，零getup帧，不能用于证明用户刚述倒地起身。坐站颈#30 P5/D20、末段请求−90°实际−85.781°、最后10帧误差中位4.263°，20/50帧机械下限裁剪；头pitch#31最后目标9.580°实际6.416°。此段颈能接近请求下限而不是单纯力弱，前期p95即时误差53.266°含快速动作/写前样本时序；load1000/current1500为原始反馈，不能换算真实力矩。分析必须用默认direction−1，已逐帧反算goals==sentTarget验证，初稿direction+1错误已离线修正未对外报告或动设备。旧倾角90→77°来自错误观测索引，已撤回；正确重算与本次对应起身见顶部纠错。已请求用户仅一次倒地起身→停止保持对应采集，遇顶住停止；未代发动作。旧378帧getup属机械对齐前native改造历史，不作当前起身验收。未调P/D/扭矩/模型/标定/EEPROM、未部署或训练。附件/平台/2026-10-11/getup-neck含原session/failure、分析脚本/analysis及固定供体源码。

> **2026-10-11 拿起放下后Roll/Pitch变化（只读诊断，未归零/改设备）：** 用户报告预备站直归零后搬动，Roll/Pitch改变、静置有时恢复。代码归零只改共享软件参考quaternion，不向MS901M写硬件加计/陀螺校准；Rust姿态仅读取模块融合四元数，前端与policy用同T×M逆×reference逆×current×M。两段30/60被动快照参考和boot均不变，排除本段参考自动被改/服务重启，不排除其它时刻。首段明显动态gyro0.197～3.498rad/s、加计模长7.304～13.210，不能据此评价静止姿态。后段60样本中44个满足诊断静置条件gyro<.03且加计模长接近g±.3（非新增保护）；quaternion旋转后的加计方向相对模块世界竖直中位0.0976°/最大1.1496°，显示当前静置段融合倾斜与加计相符，不证明整个搬动测试通过。该段实际姿态未经用户确认，显示倾斜约61°，不能当站直误差。保存参考/安装/target合成的虚拟竖直相对模块竖直约5.093°，意味着归零姿态/安装或融合参考可能有残差，会使换朝向后的相对Roll/Pitch出现耦合；不直接判安装错。最符合用户有时静置恢复的是动态融合暂态，但尚无同姿同朝向归零前-搬动-放回连续配对实测，未定唯一原因。未自动归零、改标定/IMU Flash/模型/保护/部署或发运动训练。证据附件/IMU/2026-10-11/return-zero/audit.py、两段完整快照与analysis。

> **2026-10-11 BLE控制断连已复现/分片传输修复OTA（最新，手机待验收）：** 用户重连后再次断开；runtime183231日志19:32:56先fragment-sequence-size-timeout，紧接request-sequence-gap，19:32:57通知被BlueZ结束，另19:32:29独立通知结束不能混为同一次首因。已证实有分片/会话序号故障链，未捕获该时刻完整空中片序或手机GATT数字码，不能宣称唯一无线/驱动根因。App此前所有drive均无应答写，即使加密帧超过单片容量；0.9.34/code53现按实际frame长度与MTU选择，多片强制ATT逐片确认，单片drive保留快速写；不重发动作、不放宽认证/序号/分片检查，接收失败保留具体提示。Android构建lint/API30完整回归含BLE62/后台19通过，原APK签名、公网清单验签/APK逐字节长度SHA/下载页通过，已发布OTA。主板继续runtime183231原因诊断、没有新板部署或运动/训练/标定EEPROM增益模型身份变化。用户需更新0.9.34后重连，实际控制断连是否消失尚未验收；不能称长期稳定已修复。证据ble-disconnect/reproduced-events.log、平级mobile/releases/ble-fragment-0.9.34/verification.json及日志。

> **2026-10-11 控制时BLE自行断开（最新故障，尚未定位）：** 用户报告控制时尤其易断，App仅提供“设备已断开”，没有GATT数字码。主板journal出现18:26:33/18:26:52/18:27:22/18:29:41各两条笼统request rejected，不能据此断言无线或唯一协议原因；第一次25秒被动采集持续ATT写及通知，输出被工具截断，不能声明完整断连事件审计通过。补充Rust安全分类日志（序号/分片/认证/响应大小等，不记密钥或请求内容）及订阅结束/发送失败/peer连接状态与idle时长；只增加诊断、不放宽协议或安全检查。123Rust检查通过/1忽略，ARM构建通过；空闲15卸力下runtime-20261011-183231安装，1000/1000完整只读关节帧及20在线快照，模型/标定/增益/BLE身份/网页保持，tofd和rsyslog仍masked，未发运动/训练。旧runtime175410可回滚。本次更新自身会断开连接，不能混作用户故障；已请用户重连保持停止，等待新日志与复现，不称已修复自行断连。证据附件/平台/2026-10-11/ble-disconnect及runtime183231/deployment.json。

> **2026-10-11 BLE搜索与连接已恢复（用户实机确认，最新覆盖）：** 用户明确没有断电，0.9.33仍搜不到；电脑复扫39台/无DuckLink，186和5722一度失联。只读本地设备ID扫描确认186恢复且地址未变，系统运行时间由先前10950秒变为约160秒、uptime启动时间17:53:10，期间确有系统重新启动，具体原因未确定，不归责用户断电，也未发系统关机/重启。空闲全15卸力下runtime175410完成部署，122Rust/1忽略、ARM构建、候选1000/1000完整反馈/20快照，模型/物理标定/增益/BLE身份/网页保持，旧runtime171053可回滚；通知结束断开物理peer和旧订阅generation保护已生效。部署及随后仅服务重新登记后仍无空中广播，HCI Extended Advertising Enable等命令Success、BlueZ实例1且无连接；不能把登记成功称无线成功。再次确认空闲15卸力后仅bluetoothctl控制器power off/on及重启统一服务，12秒接收4条正确服务广播（含DuckLink-f417aa，RSSI−58～−74）。用户随即确认App能搜到并连接，发现/连接已实机恢复；不改变绑定/标定，不发运动/训练，无新App OTA，继续0.9.33。controller复位加服务重登记恢复并不证明唯一固件根因或保证不复发，后台长期稳定/系统重启原因仍未验收。证据附件/平台/2026-10-11/ble-discovery/advertising-*.json、board-reboot-check.log、phone-recovery-confirmed.json及runtime175410/deployment.json。

> **2026-10-11 0.9.33再搜不到设备（残留LE已解除，代码修复待部署）：** 用户能收到几十台广播，App0.9.33无连接通知（Android13通知权限下不能仅凭无通知断言无前台服务）。主板统一Rust/BlueZ active、旧btd inactive、广告登记1实例，但一加Ace2 LE物理连接仍在；电脑10秒扫37台/无DuckLink，被动10秒ATT写/通知均0。新鲜只读确认disabled/15torque0后仅bluetoothctl断开该一加连接，未重启/动作/更改标定模型增益绑定；随后电脑36台中见DuckLink-f417aa及正确5d6b1000服务/RSSI−61，手机发现待反馈。源码发现通知结束仅drop_peer丢控制会话地址，没有Device.Disconnect，能留下物理连接；修正退出时断开对应peer，通知generation拒绝旧回调清理新订阅。未捕获原订阅结束事件，耦合恢复与源码遗漏不冒称唯一原因或一加已修复。122Rust/1忽略和ARM构建通过（新增2退出/旧回调回归）；初cargo默认examples既有缺module引用，采用生产bins检查，测试临时identity文件遗漏修正后全过。部署runtime174949在远端mkdir前因186 SSH超时终止，5722也关闭，未切服务，最后已核对仍runtime171053；已询问主板及手机搜索结果。无需新App OTA。证据附件/平台/2026-10-11/ble-discovery/long-connection-*.json、orphan-cleanup-audit.json及build/tests日志。

> **2026-10-11 后台长连接最终OTA 0.9.33/code52（覆盖上一0.9.32）：** 补齐通知栏主动断开后返回App仍抑制自动重连（手动重新连接才清除），以及背景重连完成时会话已取消则立即关闭该旧GATT，避免隐藏连接。相同原APK/清单签名，Android build/lint及API30全回归/19后台检查通过（含熄屏、Activity重建、替换会话、通知断开与抑制重连），公网清单验签/逐字节APK长度SHA/下载页核验通过。0.9.32的API26保活回归属上一版本证据。一加实际后台/锁屏/重连仍待用户确认；未连接主板、未部署/运动/训练，后台强行停止/进程被杀不保证保活。证据平级mobile/releases/ble-background-0.9.33/verification.json及最终build/API30日志。

> **2026-10-11 DuckLink后台长连接（0.9.32/code51已OTA）：** 用户要求切后台/返回不反复断连。原onPause无条件disconnect已改：BLE连接由connectedDevice前台服务及仅Application上下文的进程会话持有，Activity暂停/返回/销毁重建复用GATT/认证/序号，不保留Activity引用；前后台全部GATT事务用独立ioLock串行，通知回调不占该锁。后台只发零方向drive和接收状态，不重播技能/旧摇杆；进入后台清方向/待执行普通意图，待急停优先送出；返回采用最新状态并保持方向零。后台失联按已有自动连接设置对同已认证设备重试，2～30秒退避，不自动开启模型/使能；主动断开/解绑/通知栏断开仍释放并关闭。服务常驻低优先级通知、受限续期partial wake lock、退出最近任务保持；START_NOT_STICKY，不承诺强行停止/进程被系统杀死后仍连。后台与Wi-Fi/本地模拟不混用，通知断开不误关闭新替换会话。Android构建lint/全回归及后台17项API30（含屏幕熄灭/前台服务）与16项API26通过，原签名保持，OTA公网清单验签/APK逐字节长度SHA/下载页通过；一加实际后台/锁屏/后台重连尚未验收。仅App更新，无主板连接/部署/运动/训练，现板runtime171053不变。证据平级mobile/releases/ble-background-0.9.32/verification.json与模拟器日志。

> **2026-10-11 BLE写确认优化（已部署/OTA，手机待复测）：** 用户0.9.30报告写入约60ms、等回复20～60ms、解码7～8ms。被动HCI67请求每条仅1包（ACL dlen245），回复10通知；收到→ATT写确认median19.99/p9527.435/max32.179ms，→首通知21.168ms，→末通知30.605/p9544.308/max54.855ms。是主板HCI时间，不是手机无线时间，也不能把20ms全归Rust CPU。Rust同一认证接收入口新增Write Command支持；App0.9.31/code50仅drive/strafe_left/strafe_right且特征支持时用快速写，旧板回退，启动/停止/急停/技能/标定/电源仍原写确认。仍等Android本地写回调和匹配序号完整加密回复，无盲成功/重试；设备→主板→运行反馈显示写入方式。120Rust/1忽略、ARM构建、Android build/lint及BLE53/全回归通过，原签名/公网清单签名/APK逐字节长度SHA/下载页核验通过。runtime-20261011-171053已186安装，1000完整只读帧/20在线快照，15卸力、模型/物理标定/增益/BLE身份/网页保持，tofd/rsyslog masked、btd disabled；旧runtime165315可回退，无运动/训练。温85°C/频1416→1608MHz仅观测非归因。新手机总RTT是否改善仍未验证，写入降低可能转移到等回复，不冒称具体优化毫秒。证据附件/平台/2026-10-11/ble-latency/{request-fragment-audit,write-ack-analysis}.json及runtime171053/deployment.json，App平级mobile/releases/ble-write-command-0.9.31/verification.json。

> **2026-10-11 App耗时入口纠错（最新）：** 用户找不到0.9.29明细，核查实际仅首页linkMetrics且compact隐藏/maxLines2截断，先前“设备页已显示”描述错误。0.9.30/code49已将写入/等回复/解码/MTU放到设备→主板→运行反馈上部，可滚动且不截行；新增640×360实际页面回归与截图通过，截图35/167/2ms为mock非主板测速。App构建lint/36设备全回归、原APK签名和OTA公网校验通过；不改传输/主板，无连接主板/运动/训练。200ms峰值新手机分段数据仍待用户，证据平级mobile/releases/ble-timing-ui-0.9.30/verification.json及board-timing.png。

> **2026-10-11 BLE200ms尖峰/无损反馈更新（已部署）：** 用户反馈偶200多ms；disabled时15秒被动73请求：14片/约3.18KB，HCI接收→首通知median24.69ms/max31.63、→末通知median42.34/p9559.79/max117.16、发送跨度max91.33，未与手机200ms同请求对齐或证实优先级生效。同一真实15舵机数据共享表头1629→758字节、逐值无损；columns-v1显式opt-in，保留fault/online/电压/缺失/age/sequence/boot，App拒绝异常版本/表头/行长度，旧格式兼容。App0.9.29/code48新增写回调完成/等全回复/解密还原耗时与MTU（非纯无线各项）；build lint/33BLE全回归/原签名/OTA公网校验通过，120Rust/1忽略与ARM构建通过。runtime-20261011-165315已安装，当前卸力条件复核，候选999完整反馈/在线20只读快照，设置模型网页BLE身份保持、tofd/rsyslog masked，旧runtime164007可回滚；未动作/训练，压缩后的手机峰值仍待更新重连验证。证据[BLE分析](附件/平台/2026-10-11/ble-latency/README.md)及runtime165315/deployment.json，App平级mobile/releases/ble-columns-0.9.29/verification.json。

> **2026-10-11 BLE优化后手机复测（最新）：** 用户0.9.27确认往返仍100多ms，较旧300多改善。idle时12秒被动HCI采集63完整请求：回复13片约3.15KB，接收→首通知median26.36ms、→末通知41.48ms（28.85～70.66）、发送跨度15.31ms；旧27片/末286/跨度254。不是手机RTT41ms，也非严格同负载两轮；剩余无线/手机调度尚未同步分段证实。App0.9.28/code47新增CONNECTION_PRIORITY_HIGH请求、系统可调整/拒绝，构建lint全回归/原签名/OTA公网验签长度SHA下载页通过，实际优先级改善待手机更新重连。无需新板部署，未动作/训练/改配置；证据[复测](附件/平台/2026-10-11/ble-latency/optimized-analysis.json)，App平级mobile/releases/ble-priority-0.9.28/verification.json。

> **2026-10-11 BLE往返优化已部署（最新）：** 用户授权优化300ms。Rust移除通知分片固定sleep8ms，保留有界单回复队列/最大帧、顺序发送/yield及认证序号，通知接口无端到端确认，不冒称有发送确认背压；新App compact=true可剔除详细control诊断和已缓存同revision/boot标定，故障消息/feedbackHolding/状态/姿态/本次增益、15舵机与IMU/系统资源保持，旧App仍完整回复/握手仍完整。119Rust测试通过/1忽略、ARM构建、App0.9.27/code46构建lint/26BLE与全回归，OTA原签名/公网验签长度SHA/下载页通过。用户确认停止卸力后runtime-20261011-164007安装，独立候选20秒999反馈全完整，在线20只读快照，模型/标定/增益/BLE身份/网页保持，tofd/rsyslog masked、旧btd disabled；上一runtime155501可回滚。未运动/训练；部署采集温85°C/频408MHz为实际只读观测，不称CPU性能已优化。新版手机连接后延迟/丢片复测仍待用户，未把代码改善冒称实测新毫秒数。证据[deployment](附件/平台/2026-10-11/runtime-20261011-164007/deployment.json)，App平级mobile/releases/ble-fast-0.9.27/verification.json。

> **2026-10-11 BLE300ms被动实测归因：** 用户当前一加Ace2已连接，12秒只读btmon按ATT写请求统计29完整往返：每回复27通知/约6.4KB含分片头，主板HCI接收→首通知median31.14ms、→末通知285.99ms，首末跨度254.03ms。Rust每command带完整状态/标定/诊断，每通知固定sleep8ms；27片之间名义208ms人为等待，App必须收齐再发下一command，是300多ms的明确贡献项，不是BLE固有延迟。HCI时间非手机接收/空中时间，首31ms不能全称计算耗时；纯小包往返/实际连接间隔仍未测。本轮只读不发GATT/运动、未更改代码参数/重启部署/训练。证据[ble-latency](附件/平台/2026-10-11/ble-latency/README.md)。

> **2026-10-11 一加扫描0/0与传统扫描对照（最新）：** 用户0.9.25反馈收到广播/附近设备均0，手机仍未修复。新只读186检查统一服务active、btd inactive、1广告实例；电脑扫描仍见DuckLink-f417aa及5d6b1000服务，RSSI−53。App0.9.26/code45恢复原legacy=true扫描，保留全量接收/应用内筛选/接收统计，排除上一版扩展扫描参数；该参数尚未证明唯一根因。build/lint、模拟器全部回归、原APK签名和OTA公网清单验签/长度SHA/下载页通过，一加实际发现待用户更新确认。已请用户开系统定位/重开蓝牙排查，未获结果；无新增权限要求。本轮只读不重启板端，不发动作、不训练或改模型参数。证据平级mobile/releases/ble-legacy-0.9.26/verification.json。

> **2026-10-11 BLE手机发现仍失败/App兼容修复（最新覆盖）：** 用户确认一加Ace2以前能搜到，但停旧btd/重新注册广播后手机仍搜不到；电脑发现恢复不能作为手机已修复或唯一根因结论。DuckLink0.9.25/code44已发布OTA：改全量扫描后应用内服务UUID/严格DuckLink广播名筛选，支持扩展广播并显示广播/附近设备计数；原GATT认证绑定/权限要求保持。构建lint和模拟器21BLE/全套回归、原APK签名、公网清单验签及APK长度SHA/下载页通过；手机无线发现仍待更新后确认。本轮未新连接或修改主板、未运动/训练/改模型参数，前轮btd disabled保持。App证据平级mobile/releases/ble-discovery-0.9.25/verification.json。

> **2026-10-11 App搜不到设备（板端广播已恢复，手机待验收）：** 用户反馈搜索列表为空且无权限/错误提示。186:22主板runtime-20261011-155501、统一Rust服务和BlueZ在线，但旧供体btd同时广播duck-ded7/6f5d2a10与DuckLink-f417aa/5d6b1000，同一控制器CC:C8:37:3C:97:8F在电脑扫描回调中交替名称/UUID，形成发现干扰；不能仅凭电脑扫描断言手机唯一根因。停用并disable btd后电脑暂时也收不到统一广播，确认无活动动作后重启microduck-observer重新注册（PID3914→4522）。按App服务UUID过滤已搜到DuckLink-f417aa，随后15秒全量回调6次DuckLink/0次旧UUID，控制器仅1个广告实例；btd disabled/inactive，统一服务active/BLE ready。旧robotd --fake --no-policy未改，蓝牙身份/绑定不改；15舵机使能及目标寄存器、物理标定保持，未发运动或卸力指令、未训练/更换模型/更新App。已请用户重新搜索，手机发现与连接结果尚未确认。证据[ble-discovery](附件/平台/2026-10-11/ble-discovery/README.md)。

> **2026-10-11 嘎一下前后端/App（已部署，最新覆盖）：** 用户要求张嘴/闭嘴合并，点击一次张闭嘴并由鸭子主板扬声器叫一声；命名“嘎一下”。网页及DuckLink0.9.24/code43发送单次quack意图，Rust串口owner负责30°张开/−5°闭合，约550ms后闭合；模型循环不阻塞推理，闲置沿原单关节使能与反馈规则，先核对开闭目标均满足本机编码/EEPROM限位。会话序号拒绝重放/并发重复，技能执行/反馈暂停拒绝新请求；停止/卸力优先，传感器暂停中止叫声，恢复仅闭嘴不重播。旧mouth接口保留兼容但UI已合并。音效为本地合成0.55秒单声，Rust嵌入资源并通过固定aplay参数播放AIC3104，后台有界子进程，不写EEPROM或改模型增益。添加audio组及仅controlC1/pcmC1D0p声卡设备访问，混音器未修改。118Rust检查/1忽略、ARM构建、59网页检查/构建、App构建lint和全套模拟器通过；App原APK签名保持，OTA公钥签名/长度SHA及下载页核对上线。runtime-20261011-155501已186:22部署，独立release沿用当前模型设置；候选20秒999帧完整、15卸力，在线20快照及全部网页HTTP哈希通过，标定物理值/原文件SHA、模型/增益/BLE身份保持，tofd/rsyslog仍masked。服务有重启；初次打包缺测量脚本/可选WS依赖已修，整calibration快照含新bootId的比较误判已改物理字段和文件SHA并回滚后重装；quack路由白名单补入并增加回归测试。通用runtime overlay部署器保留当前模型，不装机器人Python服务，测量脚本在切换前从release删除；标准库WS后备不安装依赖。音频按同用户/音频组/设备沙箱单独播放exit0、722ms，在线非法会话quack返回409且音频权限预检通过；未发真实嘴部或其它运动指令，听感与同步张闭嘴仍需用户现场验收。证据附件/平台/2026-10-11/quack及runtime-20261011-155501；App见mobile/releases/quack-0.9.24。

> **2026-10-11 前端仅保留平衡行走（已部署，最新覆盖）：** 用户追加删除站立保持/定速行走入口；ModelRuntime移除模型选择区、定速选择及对应前端分支，仅保留平衡行走、轮盘速度和坐下/站起/倒地起身/拾取/翻滚/嘴部操作。后端模型文件与API保持。59前端测试与生产构建通过；release frontend-20261011-153116已通过186:22部署，网页全部资产HTTP哈希核对，主PID1020未变、无重启/运动，标定/模型/增益保持。证据见附件/平台/2026-10-11/frontend-20261011-153116/deployment.json；本次为静态资产与只读部署验证，未做实机动作验收。

> **2026-10-11 前端模型控制精简（已部署）：** 用户要求删除手动控制区/界面演示、只保留实机模型并删除XgoDuck展示信息。ModelControlPanel直接渲染实机ModelRuntime；旧预览键盘/摇杆/头嘴模拟逻辑及modelControls模块/专用测试删除，独立全部使能/静态站姿ServoActions区及组件删除，关节标定/反馈区保留。实机入口显示平衡行走/站立保持/定速行走；清理原版XgoDuck、旧P/D/EMA及后退/转向×2说明，兼容后端policy key保持，轮盘/速度/技能/嘴/停止/卸力与只读检查不改控制合同。59前端测试和生产构建通过，本地浏览器核对入口及禁用状态，截图见附件/平台/2026-10-11/frontend-controls；主板独立release frontend-20261011-152554已安装，网页资产逐项HTTP哈希一致，主PID1020未变、无重启/运动，模型/标定/增益/BLE身份保持。新增可复用tools/deploy-frontend.py只更新静态资产，原子current切换并保留旧哈希资源兼容打开的标签。5722映射上传/握手超时未切换，改走186局域网22成功；上线后浏览器直接导航超时，未冒称主板浏览器交互验收。此为页面与只读部署验证，未执行实机动作。

> **2026-10-11 用户要求旧代码直接删除（源码清理）：** 删除189个过时Python文件：118个旧工具、33个旧部署器和38个实测附件Python源码副本；另删除27个已被当前kissqy对照替代的旧Rust/Arduino/C头文件副本。没有另建代码归档；旧staging中的脚本/缓存和本地旧部署包同步删除。原始实测JSON/串口数据/结果、当前kissqy固定提交Rust源码及运行模型保持。当前仅12个必需Python打包/部署/只读验证/测试脚本，Wi-Fi/电源辅助仍Rust；辅助部署入口参数化唯一release并增加package-only，不依赖旧staging，候选验证使用当前Python解释器而非旧venv。历史记录中的已删除脚本引用仅描述当时操作，不再作为可执行入口。删除清单见附件/平台/2026-10-11/code-cleanup/cleanup.json，当前入口见tools/README.md和deploy/RUST.md。12脚本语法检查、自包含离线包检查、114Rust测试/1忽略、10影子推理测试通过；此次不连接主板、不发运动、不改模型/标定/增益。

> **2026-10-11 板上机器人服务全Rust迁移（已部署）：** 用户要求除一次性脚本外机器人服务尤其主板运行部分全部Rust。Wi-Fi管理和受限电源辅助已改为Rust src/bin/ducklink-wifi.rs、microduck-power-helper.rs，安装独立/usr/local/lib/microduck-services/20261011-service-rust-v1，主Observer仍原kissqy-native-v2进程未重启/升权。Wi-Fi原Unix socket/peer UID验证、异步连接/关联+IPv4后保存、netplan原子更新及生成失败字节回滚保留；密码仅走wpa控制socket，不进argv/日志/错误。电源只接受两个固定动作，仍systemd延迟5秒；原活动Python辅助源删除并归档回滚，仓库deploy/power-helper.py删除，打包包含ARM helper。114Rust测试通过/1忽略，含Unix传输清理、标准PSK向量、输入校验、错误隐私、生成失败配置回滚和权限测试；ARM构建通过。板上Wi-Fi关联和netplan字节未改、模型/主二进制/标定/持久增益/BLE身份保持，20快照15舵机在线卸力零missing，两个服务active，电源模板ExecStart为Rust，进程检查无Python；tofd/rsyslog仍masked。首次SSH超时未执行，重试因动作中预检拒绝；只读确认disabled后安装，用户随后回复已停止。无训练/运动/实际切网/关机重启，真实网络切换及电源动作尚未现场验收。系统netplan工具不属于机器人服务；本机和部署/诊断/实测附件的一次性Python脚本保留。证据附件/平台/2026-10-11/service-rust/{deployment,source-hashes}.json及tests.log，回滚目录/var/lib/microduck-observer/service-rust-backup-20261011。

> **2026-10-11 拾取不张嘴核查（只读）：** 用户反馈拾取下蹲时嘴不张。当前native-v2拾取模型14维输出不包含#34嘴；上次kissqy对齐移除了旧自动开合脚本，会话嘴意图初始化−5°，native循环只消费独立pending_mouth，所以没有自动张嘴目标。另确认本机queue_mouth要求activeSkill=xgoduck，网页嘴按钮也受行走/空闲门控，拾取期间无法独立张嘴；供体robotd嘴意图按ready/非跌倒/传感器有效条件执行，没有仅行走限制，是仍未对齐的接口差异。主板只读快照及最后会话保存附件/平台/2026-10-11/pick-mouth/；最后会话若无pick帧不能冒称实测嘴故障。没有发动作、训练、部署或改标定增益。

> **2026-10-11 kissqy核心策略全面核查与同步运行部署（最新覆盖）：** 用户要求模型及处理参照kissqy，并明确IMU轴向/机械零位已校准、3D与实物运动一致；采纳该反馈，不再把未校准当主要原因，不修改标定。固定上游94328f5b43c8ac825c90f0e564e7d1b926cd770d五模型逐字节哈希核验；发现翻滚仍为旧XgoDuck 8adb，已替换供体roulade 8fdaf12/1秒/无额外EMA，getup a481、walk1cbf、sitstand1c949、pick2b69权重保持。新kissqy运行器同步50Hz读→组观测→推理→写，一帧一次推理/发令，61维顺序/归一化图/raw prev_action/原生速度一采样延迟保持，切换不重复推进速度历史；起身直接model P5D20上力、嘴P10D20，保留41，不再先32/40；HOME两秒直线；电压按源码默认true/7.4V、15舵机均值每秒EMA新0.1，闲置/动作跨会话连续，再7.4/clamp(V,6,9.5)只作用动作；去掉后退/yaw隐藏2倍倍率；嘴闭−5/开30，不随pick自动开合。起身可接管坐/站/pick/roulade；直接启动的起身300个20ms有效tick结束回HOME保持，不再倾倒时无限自选getup；策略内起身结束交回walk，仍倒地由本机已有跌倒规则卸力，不伪称成功。release20261011-kissqy-native-v2已5722安装，网页更新；107Rust检查/1忽略、62网页检查/构建、7份ARM纯推理fixture（obs误差0/output≤1.2e-7、152×14机械公式目标）、378旧实机同输入重放raw误差2.38e-7通过。20只读快照零缺，15卸力；标定物理值/15寄存器/持久增益/BLE身份及其它模型保持，tofd/rsyslog仍masked，机械-v1完整可回退。App0.9.23/code42已OTA验签/长度/哈希/原APK签名与模拟器14坐站起身门控检查通过。保留本机外置IMU传输/本机reference-direction、EEPROM边界、真实fault/4～8.4V/反馈与IMU暂停和跌倒卸力处理、用户持久增益；供体现机保存的servo-test/电压开关/torque cap不可从源码推断，未声称其现机每个配置一致，也非整套robotd所有外设功能复制。尚未新实机运动验收，不能承诺能起身；没有训练或自动运动。首次v1因Windows哈希路径分隔符在服务切换前拒绝，修正为POSIX后v2安装；期间两次上传SSH超时未重复动作。证据附件/平台/2026-10-11/kissqy-native/{audit,deployment,source-hashes}.json及上游五组模型合同、构建日志。

> **2026-10-11 机械修复后真实倒地起身已捕获（最新覆盖/纠错）：** 用户完成倒地起身→停止，独立trace getup-1791685669674.json保存；378帧，a481d9模型，7.760墙钟/7.560有效秒，全程getup，结束卸力，无模型打架。倾角87.08→最好80.87（0.3秒）→83.27，6秒83.23未直立；全部机械目标无越界/同观测ORT误差2.38e-7rad，6.8～8V、零missing/checksum/servofault。本次推理48.46/反馈57.18Hz、间隔median20/p9529.92ms、异步结果age median10.39ms；前轮45.24/59.43属于混合会话非本次。首2秒快速目标跳变/跟随偏差，最后2秒target帧跳p95≤.26°，12/14跟随p95≤1.5°（neck6.94/headroll2.20），负载raw仅neck最高109其余≤43，却仍83°倒地，不能仅归因力量/P不足或持续堵转。新确认代码complete后倾角>55仍选择当前getup，next==kind过滤导致持续推理，本次超6有效秒继续78帧直至用户停止；前轮“6秒总切walk”不准确，本条纠正。供体卸力来源home_recovery窗口结束返回HOME，执行仍不同。优先核对IMU轴/机械零姿/接触装配对应并对齐运行增益上力、时序和结束流程；不直接复制供体zero/direction或默认P3D0/681。证据附件/平台/2026-10-11/getup-after-mechanical/最新起身分析.md、new-getup-analysis/segments/aligned及原trace。仅只读采集和离线分析，无训练/部署/自动运动或参数修改；停止训练仍有效。

> **2026-10-11 机械修复后起身继续失败/外围差异核查（用户反馈+只读，最新覆盖）：** 用户说仍撑不起，两按钮均点过准备再试。当前mechanical-v1限位生效；最新保存会话为walk+坐站，无xgoduck_getup，瞬时坐站stand；三轮45秒378+381+380快照全failed卸力，不能冒称已捕获新起身或将行走跌倒故障套用起身。已确认剩余执行差异：供体50Hz同步read/model/write vs本机100Hz目标发令读取+异步50Hz推理（混合会话实际推理45.24/读取59.43，不是新起身频率）；起身上力供体先model P/D vs本机先P32D40保持再P5D20；供体默认电压倍率7.4/clamp(filtered V,6,9.5) vs本机固定1（供体现机开关未知）；供体保留41 vs本机置0；供体倒地LB能接管其它技能，本机queue/App限制；卸力来源供体结束回HOME，本机6有效秒切walk。模型/基础HOME映射/机械限位已核验，不将默认管理P3D0/torque681当供体实机值，本机model5/20/I0/1000已读。两机zero/direction/IMU数据不同，实物轴对应未核实，不能直接复制。详见附件/平台/2026-10-11/getup-after-mechanical/执行差异对照.md及runtime-differences.json/快照；等用户新起身记录再隔离因果，未训练或修改部署/参数，停止训练仍有效。

> **2026-10-11 kissqy机械限位与执行已部署（用户明确授权，最新覆盖）：** release20261011-kissqy-mechanical-v1已主板5722安装，仅更新Rust二进制。15关节声明/手动控制/模型目标统一供体保存机械范围：右yaw−30～25、左yaw−25～30、双roll±22、双髋pitch/膝±90、双踝±70、neck−90～70、headpitch±90、headyaw±170、headroll±20、mouth−5～30。模型先裁机械角度，再用本机reference/direction及4096编码换算，round与供体一致；不跨圈、不用编码饱和改目标，编码越0～4095或有效EEPROM边界时拒绝整批发令。手动超范围请求仍拒绝；嘴模型接口接受−5～30，App预设开/闭仍30/0。raw prev_action/反馈/模型归一化、各技能执行时长与独立增益保持；旧训练合同/3D模型XML不冒称机械范围、不重写权重。103Rust检查通过/1忽略，ARM构建及主板五技能纯推理fixture通过；另外保存实机起身152帧×14目标按供体机械裁剪公式独立生成expected编码，ARM逐值核对全部通过，无UART访问。模拟候选15项HTTP限位核验后，空闲全卸力状态切换，标定物理值、15舵机寄存器、BLE身份、所有模型/网页与持久增益保持，20只读快照零缺，服务active；tofd/rsyslog仍masked。旧kissqy-pick-getup-v1完整保留可回退。没有训练或自动运动；这是发令一致性和只读部署验收，不代表起身已实物成功。证据附件/平台/2026-10-11/kissqy-mechanical/{deployment.json,source-hashes.json,kissqy-mechanical-tests.log,kissqy-mechanical-arm.log,policy-saved-getup-targets.json}；上一角度核查的未执行限位结论属修复前。

> **2026-10-11 kissqy起身角度限位核查（代码/归档标定/既有实机trace，最新覆盖）：** 角度限位没有完全一致，不能将相同模型权重解释为相同执行。平台声明表与供体保存机械限位15关节中8项不同：右yaw−25.78～12.61 vs−30～25、左yaw−12.61～25.78 vs−25～30；双踝±90 vs±70；neck−135～60 vs−90～70；headpitch±105 vs±90、headroll±25 vs±20；mouth0～30 vs−5～30（嘴非14维起身输出）。供体训练XML亦不是机械表：双踝±90、neck−90～60、headroll±25。更关键：当前kissqy_stand未启用bounded_targets，policy.rs只裁编码0～4095与有效EEPROM边界；供体feetech target_raw先裁保存min_rad/max_rad，再转编码并拒绝编码越界。既有152帧已发目标左hiproll−82.62～−26.72，全部越±22；右yaw最大46.32、左hippitch最小−118.04、右膝最大93.43、neck最大115.40、headyaw最小−179.47、headroll最大28.74，明确实际发令差异。供体标定是stand-contract中归档源b56de589…，不是当前供体实机回读；本证据未包含本板全部EEPROM边界，不推断统一0～4095。统计含0.05°编码舍入容差；不证明这是撑不起的唯一原因或套限位保证成功。详见附件/平台/2026-10-11/getup-compare/角度限位对照.md与angle-limits-audit.json/审计脚本/源hash。仅离线只读核查和记录维护，未发运动、训练、改代码执行/部署/标定EEPROM增益；停止训练要求仍有效。

> **2026-10-11 kissqy倒地起身失败对比（用户反馈+只读实机证据，最新覆盖）：** 用户明确kissqy/Microduck-kissqy能动但撑不起来。板release仍kissqy-pick-getup-v1；GitHub提交94328的stand.onnx、本地与主板xgoduck_getup.onnx完全同SHAa481d9f211f3…，HOME+raw倍率1/无EMA、raw history和速度反馈规则保持，本次P5/D20及torqueLimit1000。供体native默认训练P/D20与保存servo-test覆盖/R17默认3/0/681必须区分，不冒称已读供体实机设置。最新起身152帧3.04有效秒，倾角86.88°→最好77.21°→111.21°，终态disabled/卸力，无自动切换/模型报错/缺帧/舵机fault，电压6.2～8.0V；同观测ONNX重放误差1.19e-7rad。头yaw/左髋roll/头pitch等目标位置偏差p95约74.85/47.00/43.49°，多关节负载1000，#32七帧裁剪且末目标−179.47°，训练头yaw±170°；这直接显示实机跟随偏离，不是唯一机械卡死/PD根因证明。另存policy-fall是更早的行走xgoduck倾角84°，不能套到本次起身；起身跌倒检测关闭、没有到6秒窗口，不能算完整6秒失败或保证延长即成功。原ec665仿真结论属历史不适用a481。详见附件/平台/2026-10-11/getup-compare/对比结论.md及原trace/fault/analysis/固定供体源码与模型。未训练/部署/发运动/改标定EEPROM/增益/保护或重启；用户停止训练要求继续有效，自动优化检查此前已删除。后续须区分接触/负载/传感器及训练物理，不能直接加P或让顶住时硬撑。

> **2026-10-10 相邻帧/平滑覆盖CPU审计完成（离线，最新覆盖）：** 独立sit-temporal-coverage-audit-20261010 PID3956052已完成/job0/sourceHashes保持；无GPU/optimizer/训练/硬件访问。16条成功baseline轨迹构造6352对20ms相邻输入，严格同trial/同0～4、8～10、14～16秒窗口不跨断点；原real525/pairs517保持。raw反事实144组逐trial/关节p95/max已保存，stress3成功例首次2～3秒膝raw p95均值baseline15.740/13.126度，Critical15.650/13.079、control15.639/13.094；Critical平均输出漂移降低却相邻漂移p95左/右.291/.280度，高于control.197/.221，不可将均值拟合当抖动修复。CPU三actor同512对无放回seed73109，正规MLP含normalizer逐值保持、anchor和导出ORT parity通过、保存完整梯度向量197788参数：原real-temporal与关键raw膝top5%平方帧差proxy梯度cos约.090/.092/.091；关键sim绝对平滑cos约.741/.743/.742，但其基线梯度也非零且会压低正常成功动作，不能凭此直接加权训练/压平全部反馈。关键保留梯度cos control−.016/Critical−.039；teacher-delta梯度很小且cos−.048/−.090；仅本512对冻结输入局部方向非PPO更新/独立验收/实际闭环frame门，也非唯一故障原因。完整独立root证据SHAf7fdc7f8cc5c239567c0eb776ea1acc78caf275926803be430dd9091a15b14ba下载核验解压，全部manifest hash核验、temporal-coverage-final-audit.json；本地后续补充EMA.45+HOME+clip递推冻结反馈target统计，baseline与真实已捕获target最大容差3e-7rad通过，局部新增统计/源码hash另保存，重建NPZ四数组与远端原输入逐值一致（zip字节不假称同hash）。这是冻结baseline反馈的action递推，不是新动力学回放/实机PD。当前无活跃训练/合格候选，不重复3956052/CriticalReplay/旧case。下一先在同连续缓存和三actor做可微EMA+HOME+clip的真实target帧差proxy梯度，比较real-temporal/关键平滑/teacher-delta方向，raw proxy不能当实际target；必须保留跨帧图/正确初始filtered、基线回放target数值核对，控制内存并保持参数normalizer/源hash，无optimizer更新/新GPU。确认影响实际target且不压坏示范动作后才设计单项小预算训练，先测试64×5smoke正规导出normalizer，勿盲100轮重复Temporal100/ProcessV2等失败。所有最新平台技能/二进制/无EMA合同/独立增益与tofd和rsyslog masked保持，旧EMA候选不可覆盖当前kissqy入口，仿真非实机验收。

> **2026-10-10 CriticalReplay20完成且未合格（离线，最新覆盖）：** launcher3847932已结束/job.exit0，8检查、64×5smoke与1024×20正规导出通过，trainseed57121，Adam40/160、normalizer逐值保持/参数finite/std.099879，全部sourceHashes保持。完整独立root归档SHA227bab3ed59b4be251f1b0d77fb46068d49805d7738ca179bbd296bc420b72c6已下载核验解压，critical-replay-final-audit.json。四条件27/3/13/12、相对baseline无成功丢失，但stress多余行程增加2.366%、左膝p95帧跳28.788→30.999（+7.68%），过程/frame门失败；相对同20轮WeakProcess control丢5个stress成功例、行程增加10.70%，不部署、不加轮、不扩大holdout。8111/9001已经是拟合数据，这80例仅in-fit；恢复基线成功不能冒充独立验收或改善。新增CPU相同baseline观测反事实16例全帧对比：CriticalReplay/原control首2～3秒raw RMS.2941/.3890度，保留pool.2341/.2640度，全程.2153/.2162度；覆盖改动确实降低这些拟合状态上的输出漂移，却没有降低闭环多余行程和帧跳，不能以raw误差较小宣布姿态更好或唯一原因。诊断源audit-critical-replay-final.py首次整批ORT被固定batch1拒绝，改为逐帧后完成，无模型更新/仿真重跑/硬件访问，critical-replay-output-drift.json保留全16例统计。当前无活跃训练/合格候选，所有已完成旧case不重复。下一先用缓存baseline全帧核对各关节相邻输出差与漂移差、关键过渡尾部而非均值，审当前real-temporal与关键sim连续帧覆盖及其梯度是否指向实际frame门；只CPU冻结模型诊断不更新参数，不把反事实帧差等同闭环或实物响应。证据支持才设计单因素小预算修复，先测试64×5smoke正规导出normalizer，不盲100轮改系数/重复已失败CriticalReplay/FullBatch/CriticWarm/ProcessV2。CPU修订dof-id-friction-force-v1必须保持，旧物理数字不可混配。平台仍按最新observer记录保留kissqy-pick-getup-v1所有技能/二进制、坐站1c949无EMA/站起1秒回行走、新技能P5D20/行走持久P6D20、独立增益/标定EEPROM/BLE、tofd和rsyslog masked；旧EMA.45/clip-v3候选不可覆盖当前无EMA入口，诊断非当前板结论。仿真非实机验收，无主板访问/自动运动。

> **2026-10-10 闭环分叉证据与CriticalReplay20启动（离线，最新覆盖）：** SSH恢复后仅重传既有capture归档，SHA3dfe7a1b下载核验/解压、所有sourceHashes核对通过；CPU3766837共96例全qpos/result严格匹配原screen，未重跑训练/抓帧，full-batch-capture-final-audit.json。analyze-full-batch-failures.py已完成：两low失败在首次2秒起身后2.50/2.52秒出现连续5帧>1度倾角差；相同BASELINE观测含原raw-history反事实FullBatch输出此前RMS.2465/.2453度、最大1.187/1.176度（neck_pitch），control此前RMS.2141/.2096度、最大.931/.916度（left_hip_pitch）且最终成功。stress9001/3两candidate失败，分叉2.50/2.58秒，raw RMS.2916/.3028度、max2.235/2.682度，最大joint分别ankle/knee。说明权重更新在相同反馈前已有偏移，之后闭环放大；>1度仅诊断marker、成功control也会偏离，不能把marker当失败或head/leg当唯一首因，旧物理残差cap结论仍不可混配。失败控制器未部署。当前独立sit-critical-replay-control-20261010 launcher3847932已启动，GPU6启动19MiB、8测试通过；只改变offline保留示范分布，原3000 simulation保留，再从修正物理BASELINE成功的low13+stress3共16例6400帧pool（0～4/8～10/14～16秒）独立seed61007重采3000追加，均匀抽样期望50%原/50%关键过渡。real525帧/pair517逐值不变、seed.pt/teacher anchor/所有PPO奖励探索增益EMA/HOME限位/normalizer保持，WeakProcess4mini/24step/2epochs；trainseed57121，同前20轮control，先64×5smoke正规导出/normalizer/Adam40核验，再1024×20/Adam160，不重新训旧control或FullBatch。输入状态涵盖新失败附近，属于保留覆盖假设非已证明改善；关键数据源8111/9001从现在明确属于拟合数据，其80例筛选只in-fit，必须未用新seed再正式≥2seed每组200环境95%，不能当独立验收。所有源/参考回放/dataset hash锁定，dataset-audit与launch已保存，跟踪evidence/critical-replay/{progress,job.exit,comparison,job.log}不可重复启动；尚无训练效果/候选部署结论。不盲100轮改系数或压平全部反馈。保留最新kissqy-pick-getup-v1所有技能/二进制、坐站1c949无EMA/站起1秒回行走、新技能P5D20/行走持久P6D20、独立增益/标定EEPROM/BLE、tofd/rsyslog masked，旧EMA候选不可覆盖当前入口、非当前板结论。无主板访问/实机运动，仿真非实机验收。

> **2026-10-10 FullBatch抓帧传输状态（最新覆盖）：** CPU抓帧3766837已完成/job.exit0，未重跑训练；远端capture归档SHA3dfe7a1b1359846d12c143dfec319057c7c2a6e9dd48d38110589bbb2a29d0ac已生成。SSH传输再次超时，精确停止本次scp29208并保留部分文件，尚未完成本地SHA/解压，不称完整capture证据已下载。已核验FullBatch20完整证据SHA0c717c2a保持、两组失败不部署。恢复连接后只重传归档，运行已准备analyze-full-batch-failures.py做同观测反事实/分叉定位；不重训或重跑96抓帧，不访问实机。

> **2026-10-10 FullBatch20对照失败与闭环抓帧启动（离线，最新覆盖）：** sit-full-batch-control-20261010 launcher3687104已完成/job.exit0/sourceHashes保持；6检查、两分支64×5smoke正规导出/normalizer通过，seed57121/1024×20。Adam张量实际control160次/FullBatch40次（smoke40/10），std约.09993/.09996、normalizer逐值保持、参数finite；变更仅num_mini_batches4→1但aux hook与Adam次数耦合不能称纯降噪。基线27/3/13/12，control27/7/13/12却丢stress sit_cycle9001/3、膝多余行程少7.53%、左膝p95帧跳28.788→32.158（+11.71%）失败。FullBatch27/3/11/12，亦丢stress sit_cycle9001/3及low sit_cycle8111/0、9001/0，行程少5.32%但左膝p95→32.629（+13.35%）失败；相对control又丢stress4+low2、行程反增2.39%，均不扩大/部署/加轮。不能因stress成功总数相同或超过5%过程门忽略逐例退化/frame门，也不能用此否定全部大batch方法或证明唯一奖励根因。完整源/模型/metadata/四条件240回放及训练checkpoint/log/contract SHA0c717c2a下载核验解压并逐项sourceHash核对，full-batch-final-audit.json；上轮启动后SSH一次超时仅连接波动，任务已完整完成，未重复。当前无GPU训练/合格候选。下一CPU完整反馈抓帧已启动evidence/full-batch-failure-capture launcher3766837：baseline/control/FullBatch×low/stress共96例，保持原scenario顺序/seed/trials，新增capture-policy-frames，完整qpos/result逐值匹配原screen、初态不重排，记录raw/filtered/target/obs定位失败开始；无新独立验收/训练/GPU/板访问。已有launch跟踪不可重跑；待匹配证据后在相同baseline观测做两candidate反事实输出，区别初始输出漂移与反馈发散，不凭末段关节差宣称首因，不盲加系数或100轮。保留最新kissqy-pick-getup-v1所有新技能、无EMA/P5D20（行走持久P6D20）、坐站1c949/站起1秒交回行走、二进制/独立增益/标定EEPROM/BLE及tofd/rsyslog masked；旧ec665属历史不换回。旧EMA clip-v3诊断不能代表当前板模型或覆盖新入口合同。仿真非实机验收。

> **2026-10-10 FullBatch20配对对照已启动（离线）：** 独立sit-full-batch-control-20261010、launcher3687104，GPU6启动19MiB/新训练seed57121，共同seed.pt空Adam；6项RL算法唯一差异/训练及play完整配置/GAE/浮动基座DOF摩擦检查通过。FullBatch仅WeakProcess num_mini_batches4→1，1024×24步/2epochs/奖励actor-critic探索增益EMA HOME限位归一化合同保持；Adam与aux hook每轮8→2，每步6144→24576，耦合效应不称纯噪声隔离。两分支分别先64world5轮正规smoke导出/normalizer审计，再1024world20轮，按Adam step张量核对control160次/FullBatch40次（smoke40/10），检查finite与逐值normalizer。种子正规导出baseline；候选及control四条件80例各两seed8111/9001，同初态逐例匹配新DOF摩擦revision，评估相对未训练seed及20轮control的成功/膝过程/frame门；相同训练seed并不保证训练轨迹同初态，更新/随机采样消费会分叉。当前运行尚无效果/合格结论，勿重启或扩大100轮。首环境repr检查只因新闭包地址不同失败，改为仅规范地址保留其它配置后通过；首launch因GPU审计源未含CPU修正文件在hash清单前拒绝，未创建launch/PID/训练，随后补入当前CPU evaluator/BAM修正与finalizer并6测试通过才正式启动。完整sourceHashes包含训练源/seed/CPU物理/evaluator/导出/tests，跟踪evidence/full-batch-control/{launch,progress,comparison,job.exit,job.log}。本轮无主板访问/运动/部署；保留最新kissqy-pick-getup-v1所有技能与无EMA合同、行走持久P6/D20/新技能P5D20、独立增益/标定EEPROM/BLE、tofd和rsyslog masked。旧EMA clip-v3候选不可覆盖新入口。仿真非实机验收。

> **2026-10-10 J7损坏后接线计划：** 用户选择从HAT J4物理17/20/24/21引出3.3V/GND/UART9_RX/TX接原IMU，保留ttyS9；尚未接线/通信验收，详见07顶部。

> **2026-10-10 正式规模24步梯度及价值项拆分完成（离线，最新覆盖）：** sit-formal24-gradients-20261010 GPU6启动19MiB、2GAE检查，PID3539382已完成/job.exit0/源hash保持。Main0/Main99×9017/12031各1024world连续21×24步，在0/192/480步取initial/first_edge/late，共12窗口×4随机6144样本批；reward误差1.19e-7/adv重构1.24e-5/分项gradient sum2.59e-6、ratio最大偏差5.44e-5，actor含normalizer逐值保持无optimizer step。mean网络critic投影初.613～.867、切换.676～1.291、后期.938～1.050，knee−.006925～.003161；正式规模下仍非只64world偶然，但投影非奖励占比或错误证明。随后CPU缓存价值拆分PID3610451完成/2新测试/source60NPZ保持/actor保持：直接current-state baseline=-V(s)，future remainder=critic GAE−baseline（含所有未来值/截止bootstrap/timeout/reset），reward GAE，共同总std中心化；48批总gradient与GPU相对差<=6.38e-6、分项sum<=1.73e-6。baseline投影初−1.089～.635/edge.024～1.160/late−.078～1.171，future同样相消，不能将合并critic全当错误状态baseline或删除。四批mean网络方向平均cos：初.052～.147/edge−.032～.070/late−.080～.046；完整批均值范数/单批RMS .426～.599，表明本批随机minibatch方向一致性弱，时间相关/同rollout分块不等于独立噪声辨识。两完整证据SHA609bfdd0/836e10fa下载核验解压，formal24-final-audit.json、formal24-batch-directions.json与全部checkpoint/源/12rollout/48分项vector/valueparts保持。所有诊断仅独立冻结ratio≈1首更新点，非历史1024训练minibatch或第二epoch；critic normalizer采样时正常更新，梯度期间固定。当前无活跃训练或合格部署候选，勿重复上述PID。下一依据低批方向一致性做小预算FullBatch对照：只将WeakProcess PPO num_mini_batches4→1，1024×24/2epochs保持，其余actor/critic/奖励/探索/gain/EMA/限位/HOME/normalizer不变；要明确每轮Adam/aux hook次数8→2、每步6144→24576是耦合效应，不能称只降噪纯实验。先配置差异检查/64×5smoke正规导出及normalizer审计，再新seed同预算20轮control与FullBatch配对四条件80例，相对未训练seed及20轮control逐例/过程/frame门，不预设有效/直接100轮；检查已有目录/PID防重复，GPU6<200MiB才能新阶段。不重复强过程/CriticWarm旧失败。平台最新已由其它用户授权任务部署20261010-kissqy-pick-getup-v1：新getup stand教师a481…/pick2b69…无EMA/P5D20，行走用户持久P6/D20、坐站1c949无EMA/站起1秒交回新行走保持。旧ec665仿真验收现是历史模型结论，不能说当前板仍ec665，不自动换回；保留最新二进制/技能/独立增益/标定EEPROM/BLE、tofd和rsyslog masked；本轮未访问主板/运动/部署，仿真非实机验收。旧EMA clip-v3训练候选不能直接覆盖新入口合同。

> **2026-10-10 HAT硬件新故障：** 用户报告J7 IMU插座被拽掉，当前链路未验证；J5/J6同型号插座可作为移焊备件，但原接口信号不能直接替代J7 UART9。焊盘状态待查，详见07顶部。

> **2026-10-10 kissqy拾取/倒地起身已部署（用户明确要求）：** 用户确认全部卸力后，release20261010-kissqy-pick-getup-v1上线。xgoduck_pick换ground_pick SHA2b69fa681ed0…，xgoduck_getup换stand教师SHAa481d9f211f3…；P5/D20、倍率1/无EMA、raw gyro/一反馈样本延迟速度、保留同供体动作历史。拾取cos/sin周期4秒、0.7相位即2.8有效秒结束；倒地起身对应供体stand_test运行6有效秒尝试交回平衡，既有55°返回限制/故障保护保持。嘴部沿用本机0–30°及原控制。103测试通过/1忽略，主板行走/拾取/起身/坐/站五fixture通过，20快照无缺、8入口available；物理标定、寄存器、持久增益、行走坐站等其它模型、前端/BLE身份保持。行走当前用户持久P6/D20保持，新技能P5/D20；未发实物动作，动作效果待用户。tofd/rsyslog仍masked。前release kissqy-sitstand-restore-v1可回退；旧权重源码保存在deploy/staging/kissqy-pick-getup-20261010/before。证据附件/平台/2026-10-10/kissqy-pick-getup/deployment.json。

> **2026-10-10 正式规模冻结24步梯度核对已启动（离线）：** 独立sit-formal24-gradients-20261010，launcher3539382、GPU6启动19MiB，2项共同归一化/超时GAE测试通过。1024world沿用正式num_steps_per_env24/4随机minibatch每批6144，不更新actor/optimizer、不训练或导出候选；Main0/Main99两seed9017/12031，连续21个24步块，在initial/first_edge/late保存完整观测奖励值与逐batch分项梯度，核对reward/GAE/sum及actor含normalizer逐值保持，分开std与mean网络。critic normalizer在rollout按正常流程更新、每次梯度审计固定。范围仅冻结ratio≈1处独立rollout，非历史训练minibatch或第二epoch更新。已启动不可重复，跟踪evidence/formal24/{launch.json,job.log,job.exit,summary.json}；未完成无结果/根因/改善结论，全部旧失败不重复。保留最新20261010-kissqy-sitstand-restore-v1（1c949权重、无EMA/P5D20/站起1秒回新行走），保留二进制行走独立增益/起身ec665/标定EEPROM/BLE、tofd和rsyslog masked，不访问主板/发实机动作/部署。仿真非实机验收。

> **2026-10-10 缓存24步参数梯度与std/均值网络拆分完成（离线，最新覆盖）：** sit-cached24-gradients-v2-20261010 PID3480613完成/job.exit0/全部源hash保持，Main0/Main99×9017/12031，各起始、首指令切换、后期24步×64worlds，共12窗口；lambda.95/1共24个重构总梯度，分项sum相对误差最大5.92e-07，actor含normalizer逐值保持、无optimizer step/新训练/GPU/硬件访问。实际named_parameters核对197788参数，distribution.std_param前14；其总梯度平方范数占0.55～3.35%，分开后均值输出网络critic有符号总方向投影：起始.354～.728、切换.803～1.186、后期.920～.985，膝过程约−.01439～.000734。投影可负或>1，不是占比/奖励大小或唯一故障证明，价值baseline项即使期望无偏也可能有限样本方向显著。lambda1与.95总方向cos .939～.986；起始critic投影下降但切换大多升高，不支持直接宣称lambda1解决问题或据此删critic。此为64world缓存匹配上下文，不是正式1024world/24步/4minibatch（每批6144样本）历史更新；前轮512窗及保留项归因同样不能冒充原训练。完整源/checkpoints/输入/12向量/参数拆分及初失败源日志SHA2b1fe6af下载核验解压，cached24-final-audit.json。首PID3462729 launcher误引旧GPU脚本，被CUDA_VISIBLE_DEVICES空guard计算前拒绝，无GPU工作；独立v2修正完成勿重跑。当前无训练/合格候选/部署。下一核对正式样本规模的有限批方向稳定性：先审现有数据，必要时GPU6<200MiB独立1024world冻结rollout按24步分块、匹配全块共同归一化/4×6144mini-batch，取起始/切换/后期，核对reward/GAE/分项sum与actor含normalizer不变；只rollout不优化，明确非历史原minibatch。先排除64world有限样本归因再决定单因素小预算训练，不盲改lambda/系数100轮，不重复CriticWarm/ProcessV2失败。保留kissqy-sitstand-v1 SHA1c949无EMA/P5D20/站起1秒回新行走及独立增益/起身ec665/标定EEPROM/BLE；旧EMA诊断非当前板结论，不覆盖新入口合同。仿真非实机验收。

> **2026-10-10 用户要求坐站切回kissqy（已部署）：** release20261010-kissqy-sitstand-restore-v1恢复kissqy坐站1c94934c6421…权重及匹配元数据，结束85d7权重单变量对照；当前二进制、行走/其它模型、P5/D20、无EMA/倍率1/站起1有效秒及动作历史衔接不变。四主板推理核对通过，物理标定/寄存器/增益保持，20快照无缺；未发运动。tofd/rsyslog仍masked，不恢复故障服务。旧weight-ab release保留。证据附件/平台/2026-10-10/kissqy-sitstand-restore/deployment.json。

> **2026-10-10 奖励GAE/梯度分解完成及窗口纠正（离线）：** 独立sit-reward-gradients-20261010，2项超时重置/共同归一化测试通过，GPU6启动19MiB，PID3400968完成/job.exit0/源hash保持。Main0/Main99两seed9017/12031各64×512步：weighted奖励sum误差8.94e-8、normalized advantage重构误差<=7.01e-6、各项PPO raw梯度sum相对误差<=2.35e-6；actor含normalizer逐值不变，无optimizer step/训练。512步窗口中critic_bootstrap_value对总参数梯度方向的平均有符号投影.636～.746，knee_process .00011～.00238、目标加速度约−.000044～.000065；不是奖励占比/唯一根因，不据此删critic或改系数。重要适用边界：为覆盖完整循环本诊断用512步，但正式训练num_steps_per_env=24！前轮512梯度亦是独立长上下文，不代表原训练批。缓存同轨迹切成21个24步窗后critic对优势方差平均投影.022～.218（长窗.610～.715），属于优势方差非参数梯度；说明窗口选择显著影响归因。gamma.99/lambda.95名义指数衰减时间.336s仅启发，不能凭此宣布长credit更好。完整证据SHAe95c7bcf下载核验，reward-gradients-final-audit.json/完整rollout+first-batch梯度向量已保存；cached-context-audit亦保存。当前无运行训练/合格候选，不重复3400968或先前已完成诊断。下一优先使用缓存轨迹和原checkpoint在CPU重算真实24步窗口参数梯度（起始/指令切换/后期代表窗，共同std、sum对照），不必重复GPU采集；再决定信用时间等小预算单项控制。保留当前kissqy无EMA新平台、起身ec665及全部设备设置；无实机访问/运动/部署。

> 2026-10-10：仅换回Observer原坐站权重，kissqy执行方案保持，已部署单变量对照。[证据](附件/平台/2026-10-10/sitstand-weight-ab/deployment.json)。

> **2026-10-10 冻结真实rollout梯度诊断完成（离线）：** 独立gradient-v2/v3，3项梯度裁剪/方向/继承Adam缓存与不改原参数检查通过，GPU6启动19MiB。起始actor及同一WeakProcess正式run的Main0/Main99各两seed9017/12031、64环境×512步，每run128指令切换，32个正式checkpoint minibatch与16个起始minibatch；只取真实rollout/returns梯度，未训练/导出候选。起始保留梯度为0（actor与anchor相同，非代码失效），PPO原始范数4.35～5.46→裁剪1，时间平滑0.006319。Main0离线保留0.0321～0.0337/在策略0.00599～0.00629/时间0.006308；Main99分别0.1169～0.1240/0.04148～0.05347/0.005716，PPO裁后仍1。复制继承Adam一step的PPO-only/合成方向cos Main0 .999875～.999881、Main99 .998677～.998800；辅助项实际改动更新有限，但范数/方向仅本批证据，非唯一故障根因/闭环验收。actor含normalizer、critic梯度审计期间和optimizer state逐值保持；critic normalizer按实际流程在rollout采集时更新，不能把它称全程冻结。初次PID3231406因PyTorch非叶缓存deepcopy报错，源/日志保留，修复缓存脱图后v2 PID3247083/v3 PID3268921完成job.exit0/源hash保持，全部勿重跑。完整初始含失败证据SHA2d6406a4及正式checkpoints SHA6e46f0a1下载核验解压，gradient-contributions-final-audit.json清单。下一拆分各奖励的GAE/PPO梯度贡献并校验和等于原总梯度，再决定有据小预算优化，不盲调系数100轮。旧EMA.45合同诊断，非当前kissqy无EMA板模型结论；当前无训练/合格部署候选，保留新平台/起身ec665/增益/标定EEPROM/BLE，无实机访问运动。

> **2026-10-10 修正摩擦后11监督候选及独立seed复测完成（离线）：** 独立sit-supervised-corrected-screen-20261010，14检查/362源模型基线hash核验通过，11现成导出均未重新拟合/训练；四条件每actor80例严格初态配对，修正基线27/3/13/12。仅Knee2初筛无丢成功/过程少8.00%、Gate3无丢成功/少6.42%且frame门通过；新seed33013/44017基线29/7/14/15，两候选同为29/7/14/15却都丢stress的sit_cycle/33013/2，Knee2过程反增5.29%、Gate3仅少1.82%，两者失败，不扩大正式验收或部署。其它9候选逐例退化或改善不足，全部结束勿重跑PID3144296/3180284/3189519；当前无活跃训练/合格候选。完整screen SHA93c21fa9…及holdout SHAd7e75494…下载核验解压，审计supervised-corrected-final-audit.json。结论仅属旧EMA.45/clip-v3候选在修正CPU物理的回归，非当前kissqy无EMA入口实机结论。保留最新kissqy-sitstand-v1、新行走二进制/独立增益、起身ec665及标定EEPROM/BLE，未访问实机/发运动或改设备设置。下一先量化PPO/离线保留/在策略保留/时间平滑各项实际梯度范数和方向，再有据小预算训练；不要继续盲调100轮。仿真非实机验收。

> **2026-10-10 修正CPU物理后监督候选复核已启动（离线）：** 独立sit-supervised-corrected-screen-20261010，launcher3144296，14项CPU摩擦/回放检查通过；11个现成导出（Broad1/3/10、Knee1/2/3/10/30、Gate1/2/3）原文件SHA及61维/HOME/EMA/接触/限位/执行器合同匹配，不重新拟合或训练。两CPU worker，修订dof-id-friction-force-v1，复用相同源/模型的修正后基线27/3/13/12，四条件各候选80例严格初态配对；362项源/模型/基线报告哈希启动前及结束后核验。待全部结果，尚无合格结论/部署，旧物理失败不能替代此次结果。保留最新kissqy-sitstand-v1无EMA入口、新行走/独立增益及起身ec665，本批旧EMA候选不能直接覆盖其合同。未访问实机、不发动作/改标定EEPROM或BLE；仿真非实机验收。已有launch不能重复启动。

> **2026-10-10 CPU摩擦编号修正与重测（最新覆盖）：** 上游CPU BAM把DOF摩擦efc_id匹配关节id；浮动基座joint1～14/dof6～19导致串位，GPU用dof正确。新增scripts/bam_cpu_controller.py仅修正积分前摩擦预算，保留上游控制/电压/目标限速；CPU skill接入、summary修订dof-id-friction-force-v1、缓存拒绝旧revision。14本地/14服务器检查通过。GPU6空闲19MiB时4world实际装配595命名字段无差异；480冻结负载样本摩擦最大差0.259455→3.19e-5Nm，电机力矩差7.70e-8Nm。非完整CPU/Warp动力学或实机PD一致证明；旧CPU成功率及0.25°闭环漂移诊断属旧物理历史，不能与修正后混配。修正后重测12个现成PPO、不新训练：基线正常27/32、stress3/16、low13/16、high12/16。LowExplore初筛保全部成功且过程少5.74%，新seed12017/24029为28/8/15/15，丢stress1旧成功、过程仅少1.04%；Upright初筛27/6/13/12，无丢例/少5.74%，新seed28/8/15/15无丢例但过程仅少1.36%，未达5%门。其它逐例退化或frame门未过，全部结束不部署/不扩大正式验收。
>
> **起身新物理检查：** ec665正常/受扰/组合接触无扰各48/48、组合接触受扰47/48、原/v3坐姿各16/16；组合失败right_side/9001/0末段43.55°、目标逐帧最大0.0117°，头壳接地/左脚支撑，倾斜停住而非反复抖。contact-repair同时加入自碰撞与XML目标裁剪，非板上getup合同。严格复现六姿seed9001各4trial隔离共96例：原/仅自碰撞/仅裁剪各24/24、组合23/24，失败初态与原记录逐值匹配；不能怪某项单独有错，不据此改板权重。首right_side-only16例改变RNG前序未复现原初态，保留归档不当反证。七份完整证据已下载SHA核验，cpu-friction-final-audit.json清单；截图/接触对/逐帧隔离记录亦保存。
>
> **平台最新状态与下一步：** 其它用户授权任务已部署20261010-kissqy-sitstand-v1（SHA1c949…、无EMA、P5/D20、站起1有效秒回行走），保留该坐站/行走二进制及增益；本任务旧EMA0.45/clip-v3候选不能直接塞进此入口合同。全部标定/EEPROM/BLE身份/其它设置保持，本轮未实机运动/新部署，仿真非实机验收。所有PID3002200/3023503/3032392/3045828/3070559/3080998/3094345已结束，无运行训练/合格候选。下一先用修正物理复核尚未重测的现成监督/膝专项/门控候选，不重复拟合/PPO；仍无合格再做PPO/保留/时间平滑梯度贡献诊断并小预算优化，勿盲改系数100轮。

> **2026-10-10 分组漂移诊断最终（离线）：** group-drift-probe64例完成/job.exit=0、source-check通过，完整证据SHAfe69f74f…下载核验。仅腿部输出差±0.25°：stress8/16、low6/16，分别丢1/6旧成功；仅头颈四关节输出差±0.25°：stress8/16、low6/16，亦丢1/6且低gain丢例集合不同。两组完整反馈动态计算、未固定关节姿态，因此说明本批腿及头颈微小更新均可能破坏低gain过渡，不能归因唯一膝或头颈。head-only的stress膝行程下降约4.25%却成功退化，仍失败、不部署。两组同时有小修正时±.25°low9/16比单组6/16高，不能用各关节独立优化来假设整体必改善。所有probe均为双actor因果诊断非可部署单模型；已完成勿重跑launcher2953811/2937195。当前无新GPU训练/未部署/未发实机动作。下一步先核查尚未证明的CPU/GPU实际动力学一致性：相同低gain初态、qvel、EMA/raw历史与动作时序，对比BAM力矩/运动/接触配置并定位首偏差；现有64×80仅证明动作增益算术合同，不能冒充动力学等价。GPU6仅空闲<200MiB才做小批诊断，不干扰别人。不要再凭奖励系数或平均保留loss重复100轮。保留kissqy新行走/P5D20、起身ec665及其它设备设置；仿真不等于实机验收。

> 2026-10-10：kissqy坐站已部署，P5/D20、倍率1、站起1有效秒交回行走，未发实物动作。[证据](附件/平台/2026-10-10/kissqy-sitstand/deployment.json)。

> **2026-10-10 残差上界因果对照完成（离线）：** CPU两档四条件合计160例/job.exit=0、两policy与基线初态严格配对，±0.25°为正常28/32、扰动9/16、低gain9/16、高gain13/16；扰动仍丢1、低gain丢3旧成功例。±0.5°为28/10/3/13，扰动总数增加却丢1旧成功、低gain丢9。两档均不合格/不部署，且为双actor诊断非单模型导出，accepted=false/policySha=null/composite含两hash及cap。完整证据SHA8ea5a285…下载核验、原driver/模型源hash保持；初次wrapper失败SHA85b9e6e1…亦保留下载。±0.25°的三个低gain退化例均为坐姿起始首次站起，基线窗口倾角3.65～3.80°→probe35.20～35.57°，3.50～3.60秒开始持续偏离；当时接触总数两边同为2，不能从接触总数断言同一接触形态。输出漂移虽小仍可改变低gain过渡闭环，不能说限制平均漂移即可解决，更不能断言neck/knee唯一根因；观察到的关节轨迹差是发散结果非首个因果输出。当前分组探测CPU-only launcher2953811、remote sit-boundary-gain-control-20261010/evidence/group-drift-probe：保留完整反馈及原actor，仅分别在legs或head四关节组保留final输出差±0.25°，另一组仍由原actor动态计算；两组×stress/低gain×16共64例且保存完整policy帧，用于隔离哪组漂移触发失败。不新训练/占GPU/硬件运动/更改服务或参数；下一步待因果结果决定训练约束。保留kissqy新行走、P5/D20和其它设置，仿真非实物验收。

> **残差诊断启动状态纠正：** 初次CPU wrapper在进入仿真前缺get_outputs报错，旧PID2930671/job.exit=1/源码日志已保留residual-cap-wrapper-failed.tar.gz；已补齐原ORT输出接口，在独立evidence/residual-cap-probe-v2启动PID2937195，两档±0.25/0.5°四条件配对诊断，无新训练/GPU/硬件操作。勿跟踪旧PID或把该脚本错误当模型失败。

> **2026-10-10 17:45 BoundaryGain最终失败及早期漂移诊断（离线）：** 100轮完成/job.exit=0，正常28/32、扰动8/16、低增益2/16、高增益13/16；扰动丢2、低增益丢10旧成功例，扰动膝多余行程平均增加10.1%，不部署。64×80实际链确认kp .8～1.2/std .1427且compute不重置，EMA/HOME/clip误差1.19e-7、raw历史0；normalizer逐值保持、actor/critic finite、执行合同及训练源哈希保持。完整证据SHA d867ef9d…下载核验。原有64环境5轮smoke补测低增益10/16（基线12），丢sit_cycle/9001/1和3，证据SHA575d0589…下载核验；此smoke不是1024环境主训练的中间checkpoint，不能称同一学习曲线。冻结基线成功低增益观测上final输出RMS仅0.280°、左膝99分位差1.840°；拟合simulation RMS0.246°，平均保留损失可能掩盖局部敏感性，尚非唯一根因。逐例stand_cycle/9001/1第一坐窗口正常，下一站窗口倾角基线3.70°→35.45°，说明双向过渡失效而非初态即倒。已启动CPU残差上界因果探测launcher2930671：原actor与final读取相同完整反馈，仅将新旧输出差限制±0.25/0.5°，两档四条件各80例配对；不新训练/占GPU/碰硬件，不是单模型导出或可部署候选，不能按结果直接换服务。结果后再决定训练trust-region等改法，勿继续盲改100轮系数。板上新kissqy行走/二进制及17:36用户P5/D20保持；无实机动作/设置更改。仿真不是实机验收。

> 2026-10-10 17:36：用户要求按kissqy驱动默认将行走增益改为P5/D20，已保存/回读，下次模型启动生效，未发运动。见[舵机实测](03-舵机实测.md)及[配置证据](附件/平台/2026-10-10/kissqy-head/pd5-d20.json)。

> **2026-10-10 BoundaryGain分桶对照已启动（离线）：** 7项检查通过、GPU6启动前19MiB/0%，独立launcher2822519，服务器sit-boundary-gain-control-20261010。先64×5smoke/正规导出/normalizer以及64×80步实际增益与EMA/raw历史审计，通过才1024×100及四条件80例配对筛选。kp五桶.8/.82/1/1.2/uniform各20%，其它WeakProcess奖励/探索std/critic初始化/物理与61维接口保持，尚未完成、不部署；分桶有意改变gain分布/均值，只验证这种覆盖修复。已有失败权重保留，未发实机动作/改标定EEPROM增益/BLE，新行走release继续保留。

> **2026-10-10 冻结增益扫描完成（离线）：** 64例完整、job.exit=0、每gain两policy初态逐值配对。seed9001站起始循环各4例：基线8档全部4/4；ArrivalGate在0.8为0/4（末站34～35°）、0.82为2/4（两例约34°）、0.85～1.2各4/4（约3～4°）。显示本批微调后低增益裕量缩小；单seed非全面可靠性/实机P/D证据，跨gain初始静置亦随gain变化。完整证据SHA0a9d6a17…下载核验，逐帧JSON/NPZ完整；旧gain-curve driver小数目录with_suffix导致同model日志共用0.log/1.log（未丢结果/轨迹），已仅修正本地后续日志命名，原source/driver哈希归档保留，不重跑。下一BoundaryGain只改变WeakProcess训练kp采样：.8/.82/1/1.2/连续uniform各20%，刻意偏重低边界、平均kp略下降，故不是单纯等均值稀有度实验；奖励/探索/critic/全部动作接口与物理保持，先测试/64×5smoke+活动链核对，再1024×100严格四条件。未部署或发实机动作，保留当前新行走及增益配置。

> **2026-10-10 训练链审计通过/冻结增益扫描已启动（离线）：** GPU64环境80步WeakProcess实际kp_scale0.8094～1.1893、std0.099、BAM baseKp5、kd_scale0.9043～1.0994；compute间增益变化0，有效kp逐值等于base×scale。目标EMA+HOME+clip最大误差1.19e-7rad、raw prev_action误差0，未发现随机化被重置/动作链不一致。本批单seed诊断非实机P/D辨识，也非完整CPU/GPU动力学等价。冻结基线与失败ArrivalGate启动CPU增益扫描launcher2769666（原服务器sit-arrival-gate-control-20261010/evidence/gain-curve）：0.8/.82/.85/.9/.95/1/1.1/1.2，seed9001站起始重复坐站各4例，每增益两policy同初态，共64例；跨增益初始静置本身受gain影响，需要区分。此阶段不训练/不占GPU/不部署，曲线出来再决定边界采样等因果对照；不重复已失败PPO。实物/设置保持、后续保留新行走release。

> **2026-10-10 ArrivalGate100最终失败（离线）：** job.exit=0、完整配对正常28/32、扰动9/16、低增益0/16、高增益13/16；扰动丢2旧成功例，不部署。低增益stand_cycle/9001/0末段仍34.38°，不能将几何奖励门控的静态改善冒充闭环修复。normalizer逐值保持、finite/源核验通过；完整证据SHA45a13ebe…下载核验。下一步暂不继续改奖励，核查实际GPU训练中的增益随机化是否保持及EMA/HOME/clip/raw历史链路；64环境80步只仿真，无实机运动/设置更改，保留当前新行走release。奖励局部解/critic/探索各项未独立解决，不能预设唯一原因。

> **2026-10-10 ArrivalGate奖励对照已启动（离线）：** 11项检查通过，GPU6空闲，launcher2688946、sit-arrival-gate-control-20261010；先64×5smoke/正规导出/normalizer核对，再1024×100和四条件80例严格同初态全过程筛选。只门控参考ramp完成后的腿姿、两高度Gaussian及stillness正奖励（12～20°平滑降到0），不改actor观测/EMA/HOME/限位/执行器，也不抑制过渡前倾或删除upright/L1导向。根据静态奖励审计提出局部解假设，未证明唯一根因、未完成/不部署；CriticWarm最终失败证据已保存。无实机动作/参数改变，保留其它新行走release及独立增益。

> **2026-10-10 17:00 CriticWarm最终失败及奖励静态审计（离线）：** 20轮critic-only+100轮PPO及全部配对筛选结束、job.exit=0，正常28/32、扰动9/16、低增益0/16、高增益13/16；扰动成功总数保持却丢2个旧成功例，仍失败，不部署。critic-only actor逐值审计通过、最终normalizer保持/critic finite/源哈希核验通过，完整证据SHA c6421c2f…下载。低增益stand_cycle/9001/0末段倾角34.77°。9项几何任务奖励静态审计（速度置0、alpha=端点指令、非总回报/PPO advantage）正确理想坐站各14.25、错指令各1.09；同trial基线3.62°得13.29，CriticWarm34.77°仍得8.78，其中stillness1.77、composite仅0.00192。错误姿态未超过正确姿态，不能说奖励全局最优错误；但仍有部分到位分可拿，与20°验收不一致，可形成倾斜局部解。下一项ArrivalGate只把端点四项正奖励（腿姿/两高度Gaussian/stillness）在12～20°平滑衰减到0，ramp未完成不门控，保留upright/bootstrap/L1梯度、actor全部接口与物理。不是实机角度保护；待测试smoke/100轮严格回归。此轮未发实机动作/部署/更改标定增益EEPROM/BLE；当前平台新kissqy-walk-v1需保留。

> **2026-10-10 价值网络预校准对照已启动（离线）：** CriticWarm9项测试通过、GPU6空闲，独立launcher2586030，服务器sit-critic-warm-control-20261010。先64×5冒烟确认actor全部state逐值未变/导出；正式1024环境20轮只训练critic（Adam1e-3、当前奖励returns，主optimizer/actor辅助hook不运行），然后100轮同WeakProcess PPO，合计120轮。旧actor/normalizer/探索std0.1/奖励/动作及物理不改，区别是先校准critic，不能与更长actor训练混称。校准计数与optimizer随checkpoint保存，每次critic-only更新逐值审计actor。尚未完成/不部署；这验证一种价值适配处理，不预先宣布根因。LowExplore完整导出元数据合同/normalizer本地复核通过但闭环失败，保留。板上新kissqy-walk-v1及各自增益设置以observer最新记录为准，后续部署不能覆盖；此轮无实机动作/参数改变。

> **2026-10-10 16:45 LowExplore100最终失败（离线）：** 64×5冒烟后1024×100训练及全部配对CPU筛选结束，job.exit=0；正常28/32、扰动8/16、低增益8/16、高增益13/16。相对WeakProcess低增益0/16有改善，仍丢基线12/16中的4个成功例；扰动丢2例并新获1例，总数8不能掩盖退化。扰动两膝多余行程48.097/40.904→45.125/38.677°，平均降约5.84%，仍不合格、不部署。最终normalizer逐值保持、finite、探索std均值0.0297568rad、源哈希保持；完整证据下载SHA07508ced…核验。下一项拟冻结actor只校准critic后再同预算PPO，验证旧critic与监督修复actor/新奖励适配；未证明唯一根因。不发实机动作/改标定EEPROM增益身份；读取observer最新记录发现其它已授权更新已将release换为kissqy-walk-v1，后续任何坐站部署需重新核对并保留该新行走实现，不回退旧speed-040。仿真不是实机验收。

> **2026-10-10 kissqy行走模型替换（用户明确要求，已部署）：** release20261010-kissqy-walk-v1已上线，替换App使用的xgoduck_walk，原文件名保留兼容入口；来自kissqy/Microduck-kissqy commit94328f5，VelStand-Rough-Backlash第9599轮，SHA1cbf8730eadd…，不是前文被拒的脚滑偏置候选。HOME/14关节顺序相同，61→14、归一化内嵌。按仓库绑定模型的deployment-execution：scale0.9、头/腿新目标权重0.5/0.7、首目标直通、raw prev_action；新行走原始gyro、寄存器速度一反馈样本延迟，原共享IMU安装/零位不变。按R17高级参数默认P3/D0，新walk-model-gains.json与App读写独立，启动行走时写入/回读；切换其它技能时恢复其原持久增益并回读。模型HOME/手动增益、嘴、转矩上限1000、EEPROM和全部保护保持；未引入供体默认转矩限制681。此用户要求覆盖旧行走统一EMA0.45/P6D20，不改变其它技能合同。

101 Rust测试通过/1忽略、ARM构建通过；板上行走观测误差0/原始输出误差2.98e-8及两帧滤波目标核对通过，另起身/坐/站三fixture通过、8入口available。20快照15舵机全部在线、零缺失；切换前后舵机寄存器/实物标定/BLE身份保持，全部卸力、未发运动。主板model-gains接口P3/D0，下次模型启动生效；其它模型文件/前端/旧持久增益保持。CPU三档10秒未倒但0.4m/s明显转偏，仅诊断且BAM kp5并非实机P3D0映射，不作实机步态验收。回退旧release20261010-speed-040-v1，同时按部署记录恢复或删除新增walk-model-gains.json；原模型与源码已保存在deploy/staging/kissqy-walk-20261010/before。证据附件/平台/2026-10-10/kissqy-walk/deployment.json。

> **2026-10-10 低探索因果对照已启动（离线）：** LowExplore 7项配置/方差测试通过，GPU6启动前空闲，独立launcher2516396，服务器sit-low-explore-control-20261010。仅对WeakProcess探索std改0.02～0.03rad（load后首次act前及update后夹紧），旧actor均值/critic初始化、奖励、61维归一化/动作EMA/HOME/限位/自碰撞/增益采样保持；先64环境5轮smoke，正规导出校验后1024环境100轮及四条件80例配对全过程筛选。尚未完成最终筛选，不部署。Upright与Capped最终normalizer逐值保持、finite、完整元数据合同无差异（upright-final-contract-audit.json）；失败不能归因导出normalizer改变。已下载两组完整证据并SHA核验。此次无实机动作/部署/设备设置改变，持续检查跟踪现有PID、不重复启动。

> **2026-10-10 16:30 Upright两组对照最终失败（离线）：** 两组均64环境5轮冒烟/正规导出后1024环境100轮结束，job.exit=0。Upright正常28/32、扰动8/16、低增益5/16、高增益13/16；封顶UprightCapped为28/32、10/16、3/16、13/16。两组均丢基线成功例，不部署；后者成功总数提高仍不合格。相同低增益stand_cycle/9001/0末段倾角34.92/34.31°，未消除倾斜稳定解。封顶只将cost上限设4（weight−2），value loss31.90→1.28、reward−181.49→131.09，闭环仍退化，不能以损失/奖励好转当动作修好。normalizer保持、参数finite，封顶源哈希核验通过；完整证据下载SHA校验，位于平级RL artifacts/process-repair-v2-20261010/{upright-final,upright-capped-final,upright-diagnostic-audit.json}。下一小规模LowExplore仅对WeakProcess降低探索std至0.02～0.03rad，仍旧critic/奖励/全部物理与部署合同，准备后先测试冒烟，再100轮；价值模型与修复actor适配亦未排除。未发实机动作/部署/改标定、EEPROM、增益或BLE；板上仍speed-040-v1，仿真不是实机验收。

> **2026-10-10 16:00 接续优化已启动（离线）：** GPU6已恢复空闲19MiB/0%，Upright端点倾角奖励对照launcher2353603启动；64环境5轮smoke正规导出通过、参数finite且normalizer逐值保持，随后1024环境最多100轮正在运行，四条件配对全过程评估完成后才判断效果，不部署未完成或退化候选。只在参考ramp完成后增加12°以上倾角成本，保持过渡动作/61维/EMA/HOME/限位/物理合同；未发实机动作或改标定增益。板上起身已只读核对同一ec665权重，最新release speed-040-v1，不需重复替换。证据artifacts/process-repair-v2-20261010/upright-control/；每15分钟接续检查保持。

> **2026-10-10 用户要求试用起身模型（在线只读核对）：** 主板5722当前release为20261010-speed-040-v1，已安装的xgoduck_getup SHAec665f3121d8…与本批正常/受扰及坐姿仿真检查的actor完全相同，接口available=true；无需再次替换相同文件或重启服务。ARM纯推理fixture通过（动作最大误差5.96e-8rad、推理0.647ms），20只读快照15舵机全部在线、零缺失、全部卸力。控制状态failed，最后消息为行走倾角93.8°触发跌倒保护，不能当作起身模型故障或据此认定当前物理姿态。此次未发运动、未修改模型/服务/标定/增益/EEPROM/BLE；仿真通过不是实机起身验收，用户可在App/网页“倒地起身”入口进行现场试用。证据附件/平台/2026-10-10/getup-current-check/verification.json。

> **2026-10-10 过程优化与分项诊断最终结果（最新覆盖）：** SitStandProcess-v2的1000轮及CPU/GPU评估全部结束，job.exit=0，训练26分42秒。严格服务器同初态名义28/32→26/32、扰动9/16→6/16、低增益12/16→0/16、高增益13/16→13/16；扰动膝多余行程虽少15.2%，成功退化，selected=null，不部署。最终normalizer逐值保持、训练源哈希未改；两个最终ONNX文件哈希不同但全部initializer及3000条输出完全相等。四个各100轮对照Coverage/WeakProcess/Temporal/Sensor都丢成功例，低增益0/0/0/4（基线12/16），不能唯一归因强平滑税；另状态腿姿奖励参考StateReference100轮也为28/32、8/16、0/16、13/16，排除候选，但未证明或排除计时隐状态的全部影响。仅增动作EMA亦退化，不改服务滤波。原起身ec665…六倒姿正常/物理扰动各48/48，自碰撞诊断亦各48/48，原坐姿/当前v3坐姿额外各16/16；末段抖动小，暂不改起身权重，非实物或正式大样本验收。起身坐下专家直接组合循环失败，冻结传感器历史27例却收敛，不能归因独立历史自振。已收集四条件62条成功闭环示范（训练23250帧/独立seed留出35000帧），监督修复三档、五档冻结其它关节的膝输出修复及三档角速度连续门控全部完成且丢成功例，未部署；拟合回放更平滑不代表实机改善。新增Upright仅在奖励参考到位后惩罚大倾角，保持过渡动作，3项配置/代价测试通过，但GPU6被其它任务占用、启动前拒绝，smoke/100轮尚未运行。已建立当前线程每15分钟接续检查，资源空闲后再验证与训练。板仍sit-smooth-v1，未发运动/部署/改增益标定。用户持续优化/合格自动部署授权仍有效；证据平级RL artifacts/process-repair-v2-20261010/。

> **2026-10-10 速度上限已更新：** 主板speed-040-v1及App0.9.22 OTA已上线，App10–100%，全推前进50%=0.2、100%=0.4m/s模型指令；网页最高0.4，后退最终封顶−0.4，转向/侧移原上限保持。99 Rust/62网页及Android回归通过、20只读快照无缺；全部模型/标定/寄存器/身份/增益保持，未发运动、未实测地面速度。详见04同日记录与附件speed-040。

> **2026-10-10 过程优化与分项诊断最终结果（最新覆盖）：** SitStandProcess-v2的1000轮及CPU/GPU评估全部结束，job.exit=0，训练26分42秒。严格服务器同初态名义28/32→26/32、扰动9/16→6/16、低增益12/16→0/16、高增益13/16→13/16；扰动膝多余行程虽少15.2%，成功退化，selected=null，不部署。最终normalizer逐值保持、训练源哈希未改；两个最终ONNX文件哈希不同但全部initializer及3000条输出完全相等。四个各100轮对照Coverage/WeakProcess/Temporal/Sensor都丢成功例，低增益0/0/0/4（基线12/16），不能唯一归因强平滑税；另状态腿姿奖励参考StateReference100轮也为28/32、8/16、0/16、13/16，排除候选，但未证明或排除计时隐状态的全部影响。仅增动作EMA亦退化，不改服务滤波。原起身ec665…六倒姿正常/物理扰动各48/48，自碰撞诊断亦各48/48，原坐姿/当前v3坐姿额外各16/16；末段抖动小，暂不改起身权重，非实物或正式大样本验收。起身坐下专家直接组合循环失败，冻结传感器历史27例却收敛，不能归因独立历史自振。已收集四条件62条成功闭环示范（训练23250帧/独立seed留出35000帧），监督修复三档正在闭环筛选；拟合真实回放下降不作新实机改善。板仍sit-smooth-v1，未发运动/部署/改增益标定。用户持续优化/合格自动部署授权仍有效；证据平级RL artifacts/process-repair-v2-20261010/。

> **2026-10-10 用户授权持续优化/条件部署（最新覆盖）：** 用户离开期间要求效果好直接部署替换，效果不好继续分析优化，并检查倒地起身是否同样振荡；覆盖历史“不自动续训/部署”，不表示无人托稳时自动发实机动作。上一轮失败后新建SitStandProcess-v2：实际EMA两膝目标变化/加速度以0.05rad诊断尺度归一化，过程惩罚−0.15；真实相邻输入平滑权重0.3→3，on-policy保留2→0.2，仿真示范保留10。切换扰动采样0.25→1、reset后首次interval同步新指令；BAM增益每world单次采样替代14关节均值，恢复实际±20% P缩放/±10%反电动势速度缩放范围，仍非HD1910固件D辨识。新任务保留旧任务及v3/61维/EMA0.45/HOME/限位/原始历史；新4项配置/过程/增益检查、先前相关20项及64环境5轮/160步1022次扰动短测通过，归一化逐值不变。GPU6/4096环境另1000轮launcher1899602已启动，未完成；名义/扰动/高低增益回归及过程门后再筛选，不部署失败候选。板上哈希只读核对仍sit-smooth-v1坐站85d7…、起身ec665…。倒地起身CPU两seed×6倒姿×4试验正常48/48、物理扰动+一帧速度延迟48/48，最后3秒两膝单帧p95正常0.069/0.056°、扰动0.085/0.067°，本批未见坐站级振荡，暂不改其权重；非实机运动/正式200例验收。证据平级RL artifacts/process-repair-v2-20261010/。

> **2026-10-10 坐站过渡验证/1000轮最终结果（最新覆盖）：** 独立SitStandTransfer训练和CPU/GPU评估全部结束，job.exit=0/completed.txt存在、launcher已退出；训练26分33秒。新增切换物理俯仰扰动、一帧速度延迟和两膝2秒过程指标，修正CPU trial间BAM阻尼/摩擦残留。服务器严格同初态基线名义28/32、扰动9/16；100/500/999轮名义均28/32，扰动8/16、8/16、11/16。最终多余膝行程增加22.9%，丢失一个原成功例，过程筛选selected=null；GPU robust两seed循环17/32、13/32、16/32、20/32，保持25/32、29/32、29/32、28/32，未正式通过。归一化1000轮逐值保持、核心训练源哈希不变；3项过程门及先前20项检查通过。额外两组gyro一致性监督短实验亦因丢失成功例排除。BAM kd_scale为反电动势速度项，未辨识真实HD1910固件D20，不称实机PD已对齐或唯一根因。没有新部署/实物动作/自动续训，板上仍sit-smooth-v1候选、实物改善待独立验证。详见平级RL artifacts/sit-transfer-train-20261010/README.md与server-train/，本机早期基线不同运行环境不能混配服务器候选。

> **2026-10-10 坐站过渡验证/训练启动（最新离线进展）：** 用户要求新增仿真验证并优化训练。独立SitStandTransfer任务保留v3合同，新增切换0.8秒物理俯仰扰动及2秒过程指标；CPU修正每trial BAM摩擦/阻尼残留后严格同初态循环13/16→扰动6/16，证明新挑战能暴露退化，幅度是诊断假设非实物辨识。20项检查、64环境5轮正规训练/导出及264次扰动短测通过，归一化冻结逐值不变；GPU6/4096环境最多1000轮，launcher1662112已启动，未完成。种子为当前平滑actor，PPO低LR+行为保留/真实相邻输入平滑约束，平滑课程250/500轮生效，完整坐站双向回归后筛选，不自动部署/续训。平台仍sit-smooth-v1候选且实物改善待验证；本次不发动作或改标定/PD/保护。详见平级RL artifacts/sit-transfer-train-20261010/。

> **2026-10-10 原坐站振荡输入归因（最新分析，未改实机）：** 旧种子2a3edcb880…的97坐初帧/94相邻对穷举五输入块，重构误差1.2e-7rad；主要目标变化由gyro和关节速度驱动，典型左膝+45.33°中gyro+36.07°，下一显著反转由gyro/速度共同驱动。过渡俯仰gyro局部敏感度约33°/(rad/s)，真实gyro逐帧变化p95 2.231rad/s，比本批成功名义CPU轨迹0.827～1.017高约2.2～2.7倍；非完整训练分布证明。九个冻结传感器、只递推上一动作的150帧诊断全部收敛，不能归因独立动作历史自振。gyro/gravity变化相关0.760，无明显整体反号；仍不能排除振动/时间错位或隔离真实PD/接触原因。配置较强平滑阶段1500/2500轮，既有1000轮未达到；端点验收此前漏看过程抖动。当前板仍sit-smooth-v1候选、未新物理验证；本次全离线，无部署/动作/参数改变。证据附件/平台/2026-10-10/sit-trace/input-attribution.json及振荡输入归因.md。

> **2026-10-10 坐站平滑候选已部署（最新覆盖）：** release20261010-sit-smooth-v1，alpha_sitstand SHA85d7e1a48f…；基于实机原始输出振荡记录做300次监督梯度更新，以完整仿真轨迹保留约束+真实同指令相邻输出平滑损失修正MLP，保留原ONNX归一化图/61→14/EMA0.45/原始prev_action/限位/接触合同；不是新1000轮PPO或手工插值。五档候选中选择weight1；更强3/10在独立96例分别86/85通过，原88，已排除。选中候选与原配对16/96/32共144例，两者均134通过，所有原成功例无新增失败；仍小样本accepted=false，未称完全可靠。相同真实观测回放（拟合内样本、非新实机闭环）坐初2秒发令两膝累计行程354.82/314.76→233.77/212.25°，降低34.1%/32.6%，逐帧14维输出变化RMS降低46.8%。主板两方向ARM数值核对/8入口available/20快照零缺失，服务二进制/网页/其他模型/物理标定/15寄存器/BLE身份/持久增益保持，未发动作；上一release sit-trace-v1保留可回退。新权重实际抖动改善尚待用户再做一次坐站，不能拿旧试验冒充新模型实物验收。证据附件/平台/2026-10-10/sit-smooth/deployment.json及平级RL artifacts/sit-smooth-repair-20261010/。

> **2026-10-10 坐站逐帧原始输出最新覆盖：** 已部署release20261010-sit-trace-v1，仅更新Rust诊断与同SHA/HOME坐站历史保留，模型仍监督种子2a3edcb880…；98Rust/1忽略、9CPU回放测试及ARM构建通过，标定/15舵机关键寄存器/BLE身份/持久P/D/网页和模型保持。用户再次完成App平衡→坐→站→停止保持，保存856推理帧。坐→站历史保留已生效，但坐初期原始左/右膝输出2秒累计行程596.51/492.52°，发令354.82/314.65°，不能称抖动已修复。逐帧CPU ONNX重放误差≤2.4e-7rad、EMA误差0，支持原actor在真实反馈下振荡；双专家在同观测也振荡，单模型蒸馏/切换清历史不是已证实唯一根因。速度方向/量级无明显错误、一帧滞后相关0.983，额外延迟未改善；初态/15～35ms时延仿真未复现相同幅度。停机报告推理48Hz/反馈50Hz，不称达100Hz。证据附件/平台/2026-10-10/sit-trace/；后续离线平滑候选尚未实物验收。

> **2026-10-10 新坐站实物过程已捕获（最新覆盖）：** 用户App一次坐下再站起完成，反馈坐下初期反复抖几次。50秒500次只读采样捕获walk→sitstand_sit约8.2秒→sitstand_stand约4秒→walk→停止保持，坐/站均正确种子SHA2a3edcb880…，sit指令始终[1,0,0]、stand始终0；无模型反复切换。实时短反馈寄存器67显示初期左膝目标约41.2°→−0.3°（约0.1秒间隔），随后多次反向，踝同步变化，约2秒后收敛。稳定坐姿实际两膝约+67.5/−69.1°、目标+67.5/−67.8°，稳定段关节负载较低；完整坐段7.4～8.3V、瞬态#14负载1000，无新故障卸力、发布状态无保持且IMU暂停计数0。用户确认已坐下，不能继续称坐不下/悬空；一次成功非稳定验收。直接现象是目标振荡，模型闭环/切换动作历史清零/仿真实机时序差异尚需隔离，未证实单一根因；本次只读无新控制参数/服务修改。状态约1Hz发布，不能逐帧与反馈对齐；曲线使用新鲜寄存器67而非缓存42。证据附件/平台/2026-10-10/bidirectional-sit-inspect/{timeline.json,calibration-capture.json,trajectory-analysis.json,sit-transition-targets.png}。

> **2026-10-10 新坐站实物反馈（最新覆盖）：** 用户反馈App坐下有动作意图但未落座，实际坐下尚未验收。只读检查一个Rust控制进程/唯一舵机UART owner；代码切换销毁旧推理线程，坐下期间BLE轮盘及共享行走入口拒绝覆盖，坐下无自动返回walk。最新保存故障却为activeSkill=xgoduck、供体walk哈希5727b909…，#12电压3.8V卸力，故障前多关节负载±1000；不能冒称这是新坐下模型的故障过程。50秒500次只读采样全部disabled、零坐下帧，尚不能判定坐下失败根因；未发动作、未改增益/保护/标定/服务。需后续捕获sitstand_sit目标与反馈过程，证据附件/平台/2026-10-10/bidirectional-sit-inspect/。

> **2026-10-10 双向单actor候选已部署（最新覆盖）：** 用户选择微调前监督种子，已5722部署release20261010-bidirectional-seed-v1，仅替换alpha_sitstand权重/元数据，SHA256 2a3edcb880e88435a7ac248568859bb4f1eb487b9204db06a4aa3fb59c5ebe29。现有model-gains-v1 Rust已支持v3目标限位/坐站指令，二进制、网页、其他模型不变；旧release保留可回退。主板两方向纯推理数值校验通过、8入口available、20只读快照无缺；物理标定/15舵机关键寄存器/BLE身份/持久增益保持，重启后BLE ready、IMU streaming、全部卸力。种子CPU循环31/32是小样本仿真，非正式验收；未部署后续失败PPO或双专家组合，未发实物动作，旧坐下顶压故障仍须新模型现场验证。证据附件/平台/2026-10-10/bidirectional-seed/。

> **双向微调最终失败（最新覆盖）：** 2026-10-10 双向种子后1000轮PPO微调及GPU/CPU评估已全部结束，耗时28分35秒；最终权重再次退化，GPU nominal两个重复循环两种子均0/32，CPU两循环两种子均0/8（合计0/32），CPU站保持两组7/8、坐保持两组8/8。GPU门分类：从坐姿起身高度/腿姿几乎全失败；站姿起始下降也有23～24/32失败，多数倾角仍通过。不能把种子31/32写成微调最终结果；初始双向组合与监督种子保留，未替换它们或部署。说明现有PPO目标/优化过程未维持已学双向动作，具体奖励/探索/课程因素尚未隔离证明，不能断言唯一原因。未自动续训、未实物操作；证据平级RL artifacts/sit-contact-bidirectional-20261010/finetune1000/。

> **双向坐站优化（最新覆盖）：** 2026-10-10 双向坐站最新：相同v3合同的两轮专家按姿态指令组合，GPU正常循环96.875～100%、扰动最差78.125%；CPU循环31/32，站保持15/16、坐保持16/16。已用64000条真实组合回放初始化单actor，单actor CPU循环同为31/32，站保持14/16、坐保持16/16；43测试、5轮种子冒烟/导出通过。训练仅新增强制反向采样4～5秒/24秒episode，GPU6隔离1000轮微调launcher1051370已启动，未完成/未验收/未部署，限位/接触/顶压/EMA/PD保持。详见平级RL artifacts/sit-contact-bidirectional-20261010/README.md；旧实物坐下仍失败。

> **坐站第二轮最终结果（最新覆盖）：** 2026-10-10 objective-v2的1000轮及CPU/GPU评估全部结束，job.exit=0/completed.txt存在，launcher已退出；耗时27分36秒。仍未通过：GPU nominal重复循环两种子全0/32，robust仅1/32一组、其余0/32；CPU两重复循环两种子均0/8，站保持两组7/8、坐保持两组8/8。转换窗口表明方向与首轮相反：下降基本成功，但起身窗口height/legPose大量失败，tilt基本正常；CPU循环稳定比例典型[1,0,1]/[0,1,0]。不能把奖励上涨110.81称技能通过，也不能称仅缺训练时长。权重bc2fb7663471f4c9685b55a7b23b770e5db183d5dc3685d49721366390358c3d，未续训/部署/实物动作。证据平级RL artifacts/sit-contact-objective-v2-20261010/{gpu-nominal,gpu-robust,cpu-runtime}.json。

# 我们的实测记录

索引维护日期：2026-10-10；最近部署检查日期：2026-10-10；最近用户动作故障诊断日期为2026-10-10。这里统一保存本项目用户与助手实际接线、测量、主板采集、部署验证和故障排查记录。先读本页，再看对应主题中的具体日期。

**舵机修改前先读：[当前执行规则](03-舵机实测.md#当前执行规则2026-10-01修改控制前必读)。工作电压4.0–8.4V，不加6V门槛；不加50℃软件线；发令目标50Hz须看实际统计；最终到位容差±5°；站姿目标按官方角度范围，不恢复速度推算和1024步距离拦截。已取消55步跟随/负载300/预检8步等门槛，加速度新控制设0。旧试验脚本和历史段落不能覆盖这些已确认要求。**

## 目录迁移（2026-09-28）

记录整理：本目录从 `microduck-replica/docs/实测记录` 迁入 `microduck-observer/docs/实测记录`，主题记录、历史归档和摄像头附件一并保留。迁移前后全部 13 个文件通过 SHA-256 核对，历史归档未改写。本次仅整理文档，不构成新硬件实测，以下故障、验证边界和未解决项保持原结论。跨项目资料链接适用于本地平级工作区，单独克隆本仓库不包含这些外部资料。

## 最新状态

## 2026-10-10 坐站奖励优化第二轮（离线，未部署）

首轮1000及CPU/GPU评估全部结束：CPU重复循环两种子全0/8，站保持7/8、8/8，坐保持全8/8。主要失败是站起后不响应坐下。三项端点审计旧SIT站着3.9348/坐着4.0（非总奖励）；独立候选已改整体腿姿Gaussian、高度归一化/权重3、删除高站姿奖金并延后平滑税。接触/限位/顶压/EMA/61维合同不变。33测试、64环境5轮冒烟/导出及CPU/GPU链路通过后，隔离GPU6从头再1000轮，launcher653594，未完成/未部署。板上旧坐下仍失败；旧证据保留，静态保持及短训练非验收。



2026-10-10 修正坐站候选1000轮已训练结束：GPU正常条件站坐保持全通过，重复循环仅6.25～12.5%；扰动循环3.125～28.125%，完整GPU初筛未通过，CPU仍回放中。无自动续训/部署，实机旧坐下仍失败，见03和平级RL最新训练记录。

2026-10-10 坐下修正500轮中途初筛：GPU单种子16世界/CPU每场景2次，站坐保持全过，两种重复坐站循环均未过；抽查可从坐姿站起，但坐下命令后仍站着。继续原1000轮，未部署，不能以小样本保持结果宣称实物可用，见03及平级RL训练记录。

2026-10-10 坐下顶压修正候选：独立RL任务启用自身接触，统一训练/CPU/新v3合同Rust的EMA+HOME后目标限位并加入顶压误差惩罚；调整膝±1.25rad/髋∓0.4579rad与坐高，GPU64/CPU32静态小扰动保持通过，原较浅候选GPU失败已保留。32RL、93Rust/1忽略、ARM及5轮冒烟/导出链路通过；隔离GPU6启动1000轮初看，未训完/未运动循环验收/未部署。板上仍旧sitstand5000、坐下实机失败，保护/标定/EEPROM保持，见03/04。

2026-10-10 最新坐下实物失败：用户确认腿或头顶住仍往下压，新sitstand5000有效进度2.2秒两膝目标顶编码边界/负载±1000，整组电压3.8～4.5V，#13触发卸力；原包校验通过，卸力后8.2～8.4V。纯ONNX重现大输出，模型仿真通过不代表实机可用；先失稳/先掉压尚未确定，未发动作或改保护/标定，见03/04/07及sit-voltage附件。

2026-10-10 App主板资源/系统/部署信息与蓝牙关机重启已接入；Rust board-power-v1已经5722部署，标定/寄存器/身份/模型保持，BLE ready/电源socket active；App0.9.19/code38已OTA并公网验签/哈希。92Rust、Android小屏及回归通过，未发动作或真的关机/重启，真实手机待验收，见03/04。

2026-10-10 零位按钮/Rust全15关节0°接口已部署5722，release20261010-zero-pose-v3；在线二进制/首页JS/CSS哈希通过，标定/寄存器/身份/全部模型保持，八入口available、20快照无缺，BLE ready/IMU streaming。服务idle、15舵机仍卸力，未执行实物归零，见03/04及zero-pose部署附件。

2026-10-10 全15关节零位按钮与Rust接口完成，90Rust/62网页及ARM构建通过；部署切换前检测活动任务而暂停，板上仍hd1910-skills-v1。等待用户停止保持，未发运动/写EEPROM，见03/04。

2026-10-10 起身4750/坐站5000新模型、Rust服务和网页已部署5722主板，release20261010-hd1910-skills-v1；八入口available、ARM三数值核对通过、二进制/网页/模型哈希通过，标定/寄存器/身份保持，20快照无缺，BLE ready、IMU streaming。行走跌倒停止一并上线；未发运动，实物验收待完成，ToF仍离线。见03/04及同日部署附件。

2026-10-10 起身/坐站新训练模型已本地替换，Rust完成条件及网页等待窗口适配；89Rust/62网页、3项数值核对、模拟8入口及ARM构建/部署包完成。5722连接关闭/186超时，主板未安装，无运动或实物验收，见03/04及附件/平台/2026-10-10/hd1910-skills/。

2026-10-10 RL/observer限位本地复核：存档XgoDuck XML、RL/observer快照及网页全部14项一致，Rust范围源码逐项一致，RL 16项测试重新通过；本次未重跑Rust测试（PATH无cargo），未主板/EEPROM操作或部署，见03。

2026-10-09 用户要求RL/observer限位对齐：本地RL共14个XML变体的14模型关节已统一XgoDuck，CAD导出自动重应用；observer后端/API及网页原已同值，新增全部14项快照检查通过。16项RL检查和四方向短仿真通过，嘴仍0～30°，未训练/部署/主板或EEPROM操作，非起身实物验收，见03当前执行规则及microduck-rl/docs/server-training.md。

2026-10-09 observer/mobile废弃Python后端及旧入口本地已删（75文件），模型迁models/、7ONNX哈希/元数据内容保持；86Rust/10影子/6Wi-Fi及HTTP/WS/ARM/打包通过。仍用Wi-Fi/电源辅助与OTA工具保留；主板关机未远程清理，无新App OTA，见03/04。

2026-10-09 行走跌倒终止推理/卸力代码和ARM部署包已完成：倾角≥60°持续150ms或≥90°立即；86测试通过，起身/翻滚等动作豁免。用户已关机，未主板部署/实物试验，见03/04最新记录。

2026-10-09 统一后退×2/转向×2补偿和网页单轮盘yaw已部署motion-command-map-v1；83 Rust/62网页测试、构建/在线哈希通过，标定/寄存器/身份保持、BLE ready、20快照无缺。App0.9.18直接适用，无新APK；未实物试走，中间档/斜向组合仍待验收，见03/04最新记录。

2026-10-09 左右转向仿真：原版模型+0.5左转弱/−0.5右转明显、±1均明显，四组0/32失败；App符号反向已修复并OTA0.9.18/code37，回归/公网签名哈希通过。主板转速范围未改，实物仍待复验，见[舵机记录](03-舵机实测.md)与[平台记录](04-网络与观测平台.md)。

2026-10-09 同款XgoDuck模型后退仿真：32环境−0.2几乎驻足，−0.4实际约−0.201m/s且0/32失败；正+0.2实际+0.109m/s。模型前后响应不对称，服务负指令下限−0.2落在弱后退区；非本机实物，未改速度或部署，见[舵机记录](03-舵机实测.md)。

2026-10-09 后退排查：App/服务代码保留负值，模拟链路vx−0.2测试通过，主板ONNX对负指令有输出响应；45秒只读采样始终待机，未抓到实机后退请求，尚不能判定物理后退失败原因，见[舵机记录](03-舵机实测.md)。

2026-10-09 用户授权XgoDuck模型限位处理优化已部署20261009-xgoduck-encoder-limits-v1：模型直接反馈角度/编码饱和，取消模型训练XML角度裁剪和反馈越界卸力；80测试/ARM数值及#10越界fixture通过，标定/寄存器/身份保持，20快照无缺，未实物运动，见[舵机记录](03-舵机实测.md)。

2026-10-09 XgoDuck运行源码核对：14模型目标不按训练XML角度裁剪，只换算后限编码0～4095；未见反馈超XML角度即卸力。本机处理仍有目标角度裁剪/反馈越界停机，不能将数值一致表述为运行机制一致；未主板修改，见[舵机记录](03-舵机实测.md)。

2026-10-09 最新XgoDuck walk XML与主板在线软件限位复核：14模型关节逐项一致，嘴#34仍为用户配置0～30°（walk XML无嘴）；未EEPROM检查/改动或运动，详见[舵机记录](03-舵机实测.md)。

2026-10-09 经用户提供SSH映射只读检查主控日志：多次#10/#20模型越界；最新#10高于原版上界约0.227°且故障时速度为零，不能确认后退指令历史。采集时模型已停止，未发运动/改参数，详见[舵机记录](03-舵机实测.md)。

2026-10-09 DuckLink0.9.17/code36新增只读主板身份与运行信息已OTA，回归/断线模拟UI/公网签名哈希通过。CPU等系统指标尚无BLE回传，未主板变更，见[平台记录](04-网络与观测平台.md)。

2026-10-09 DuckLink0.9.16/code35修复已存IMU参考跨启动只读显示被误拒绝，27姿态及回归、公网签名哈希通过并OTA；未写主板标定，手机效果待验收，见[平台记录](04-网络与观测平台.md)。

2026-10-09 DuckLink0.9.15/code34三轴姿态移到底栏并恢复轮盘空间，链路指令往返计时去除App调度等待，已OTA与公网核验。未手机分段延迟实测，见[平台记录](04-网络与观测平台.md)。

2026-10-09 DuckLink0.9.14/code33新增共享标定机体横滚/俯仰/偏航只读展示已OTA，25姿态检查及小屏/高密度回归、公网签名哈希通过。手机实机角度待验收，见[平台记录](04-网络与观测平台.md)。

2026-10-09 DuckLink0.9.13/code32修复高密度横屏与字体放大裁切已OTA；两组模拟器回归、构建/lint和公网签名/哈希通过。一加Ace2两页真机效果待用户更新验收，见[平台记录](04-网络与观测平台.md)。

2026-10-09 DuckLink0.9.12/code31界面重构已OTA：暖白墨绿、三栏遥控/自绘图标摇杆/设备侧栏卡片；两尺寸回归及构建/lint、公网签名/哈希验证通过。未真实手机/实物验收，见[平台记录](04-网络与观测平台.md)。

2026-10-09 DuckLink0.9.11/code30视觉调整已OTA，构建/lint及公网清单签名、APK长度/哈希、下载页通过；未手机/实物验收，见平台记录。

2026-10-09 DuckLink0.9.9/code28视觉调整已OTA，构建/lint、公网清单签名与APK长度/SHA256及下载页通过。未真实手机安装/主板改动/实物动作，见[平台记录](04-网络与观测平台.md)。

2026-10-09 DuckLink0.9.8首页倒地起身/站起坐下动态切换、单轮盘前后与转向；640×360等两尺寸布局/手势及坐站检查通过，已OTA并公网验签/哈希核对。未主板改动/真实运动，见[平台记录](04-网络与观测平台.md)。

2026-10-08更新完成后按用户要求正常关机，请求accepted且等待约30秒后SSH/网页离线；物理电源状态未远程验证，见[平台记录](04-网络与观测平台.md)。

2026-10-08 DuckLink蓝牙/控制统一Rust已部署，旧Python蓝牙服务停用；绑定/标定/舵机寄存器保持，广播与只读Wi-Fi接口通过。App0.9.7默认无需解锁、急停点击恢复，模拟器/79 Rust测试通过，OTA公网签名哈希核对；手机BLE/实物动作待验收，见[平台记录](04-网络与观测平台.md)。

2026-10-08按用户要求持久关闭mediad：摄像头缺失/插件缓存失败导致238次自动重启反复拉起gst-plugin-scanner；已停用自启并加持久条件，试启动被跳过、相关进程消失。mediad摄像头/WebRTC通道停用，未动标定/舵机，见[摄像头记录](05-摄像头实测.md)。

2026-10-08 方向拒绝通用“动作执行中”也覆盖反馈暂停；只读快照显示IMU暂停1次/恢复1次、当前walk运行。已本地拆分提示，71测试通过，在线服务尚未更新，见[平台记录](04-网络与观测平台.md)。

2026-10-08 网页轮盘新增0.02～0.20m/s前后调速/快捷档/已接受与后端指令显示，横移≤0.10；62测试/构建与静态部署通过，未实物发令，用户拖动/步态待验收，见[平台记录](04-网络与观测平台.md)。

2026-10-08 网页控制区按2K重排为横向紧凑布局、自然高度不拉伸/不内部滚动，3D占剩余空间；59测试/构建及静态部署哈希通过，用户视口效果待刷新验收，见[平台记录](04-网络与观测平台.md)。

2026-10-08 网页3D与下方控制区改为1:1，控制区内部滚动；构建/静态资源哈希部署通过，无服务重启/动作，视口视觉待用户刷新核对，见[平台记录](04-网络与观测平台.md)。

2026-10-08 #10启动预检查拒绝：当前右髋偏航+21.71°超过原版上界+12.61°；online/fault0/失能，未发动作/未改标定，见[舵机记录](03-舵机实测.md)。

2026-10-08 IMU过期≥150ms改为暂停推理/方向归零/保持，恢复后继续，≥1秒仍卸力；71测试/ARM构建及只读部署通过，标定/寄存器/模型/网页保持。新诊断捕捉到167ms遥测发布锁等待，持锁者未定位；未实物动作，见[IMU记录](02-IMU实测.md)。

2026-10-08 XgoDuck零速度平衡因IMU姿态234ms过期立即卸力；舵机完整/无坏校验。只读另观测到177ms数据年龄，串口累计无I/O/校验错误，调度/采集根因未定；已核对XgoDuck150ms转shadow与官方3帧coast后保持，未修改或发动作，见[IMU记录](02-IMU实测.md)。

2026-10-08 官方alpha_sitstand新版坐下维持/站起已接入主板、网页和DuckLink网关；69 Rust/59网页/46网关测试、ARM坐站数值误差0，标定/寄存器/BLE身份保持。App0.9.6已OTA并公网验签哈希；未实物动作，见[平台记录](04-网络与观测平台.md)。

2026-10-08 用户确认与XgoDuck同结构，14关节原版范围已统一部署（颈−135～60°/头俯仰±105°/原版髋偏航），后端API/网页逐项核对通过，标定/寄存器保持、20快照无缺；未重放动作，见[舵机记录](03-舵机实测.md)。

2026-10-08 用户拾取动作#30越界卸力：模型要求−131.8°（原版颈范围−135～60°），本机裁至−90°后实测−90.84°触发保护；15失能。只读诊断，未取消保护或重试，当前拾取实物验收失败，见[舵机记录](03-舵机实测.md)。

2026-10-08 正常重启恢复写入，轮盘会话/错误提示修正已部署；59测试及资源哈希通过，物理标定保持、15在线，未发动作；存储根因/实物效果待验收，见[平台记录](04-网络与观测平台.md)。

2026-10-08 网页轮盘无动作排查：本地修正演示/实机入口、会话持久与指令错误显示，59测试/构建通过；主板mmcblk1写I/O error导致EXT4日志中止/写入只读，新修正未部署。标定已备份，等待托稳停止后的重启确认；未发动作，见[平台记录](04-网络与观测平台.md)。

2026-10-08 DuckLink0.9.5设备页改为横屏分类/并排操作，23项小屏布局检查及构建/回归通过，已发布OTA并公网验签/哈希核对；未手机实测/未主板改动，见[平台记录](04-网络与观测平台.md)。

2026-10-08 实机网页已部署方向轮盘/按住转向/站起入口，55测试与构建、7份在线资源哈希通过；未重启/发动作，视觉触控及运动待验收，见[平台记录](04-网络与观测平台.md)。

2026-10-08 192.168.31.186已部署XgoDuck四模型后端、网页动作/嘴部控制及DuckLink网关；六模型在线available、资源哈希/20只读快照通过，标定253/寄存器/BLE身份保持。未发动作，实物效果待验收；见[舵机记录](03-舵机实测.md)与[平台记录](04-网络与观测平台.md)。

2026-10-08 DuckLink0.9.4将遥控连接状态并入中央、隐藏顶部状态栏；小屏检查通过，已发布OTA并公网校验。未手机实测/主板更新，见[平台记录](04-网络与观测平台.md)。

2026-10-08 DuckLink0.9.3精简双大摇杆/48dp常用按钮/更多动作菜单，修复顶部裁切；小屏模拟器检查及构建通过，已发布OTA并公网校验。用户手机触控和主板动作仍待验收，见[平台记录](04-网络与观测平台.md)。

2026-10-08 DuckLink0.9.2本地整理为Xbox式双摇杆、方向键、ABXY、肩键与固定急停；中央延迟/日志保留。构建/lint/组合输入及Android模拟器检查通过，未实机动作或手机验收，App0.9.2已发布OTA并公网验签/哈希核对，见[平台记录](04-网络与观测平台.md)。

2026-10-08 DuckLink0.9.1本地去掉遥控页3D模型，增加指令往返延迟/平均/峰值/回包时间与实时日志；APK构建/lint通过，未手机实测/未发布本版OTA，见[平台记录](04-网络与观测平台.md)。

2026-10-08 网页与DuckLink已本地适配起身/拾取/翻滚，DuckLink0.9.0/code19横屏控制及张嘴/闭嘴；动作期间锁方向、完成回零平衡。主板断电，主板未部署/未动作验收；App0.9.0已发布OTA并公网验签/哈希核对，见[平台记录](04-网络与观测平台.md)。

2026-10-08 XgoDuck四模型后端代码/动作切换接口已完成，66测试/本地原生数值核对/ARM构建及包校验通过；用户主板断电要求先写程序，未部署/未实物动作，见[舵机记录](03-舵机实测.md)。

2026-10-07模型首次非零故障包隔离/只读补确认已部署，重复故障仍卸力、确认缺失暂停推理；62测试/ARM构建通过，标定/寄存器/模型/前端保持，20只读快照无缺。未新实物动作、通信根因未解决；手机0.8.0无需更新，见[HAT记录](07-HAT实测.md)。

2026-10-07 XgoDuck零速度运行#14报143并卸力，触发包多字段异常但8位校验通过，伴3坏包/缺21、23、24，疑似通信损坏漏检；后续全15在线/失能/fault0。只读排查，UART/HAT/供电根因未定，见[HAT记录](07-HAT实测.md)。

2026-10-07 21:59用户运行策略后#10范围检查卸力；原始回包仅超−30°边界约0.059°，严格边界检查触发。诊断显示运行旧hd1910-head-v5站立策略，非XgoDuck；整段仍有缺帧/校验错误。只读分析，无新动作或参数修改，见[舵机记录](03-舵机实测.md)。

2026-10-07 DuckLink0.8.0按住行走已接入并部署BLE服务；设备身份/舵机寄存器保持，38网关测试/8安卓状态检查/构建lint通过，APK局域网下载哈希一致。手机BLE控制与实物步态未验收；云端App OTA已发布0.8.0/code18，公网验签/哈希核对通过，见[平台记录](04-网络与观测平台.md)。

2026-10-07 XgoDuck原版walk已部署到192.168.31.186：新增预备姿势、零速度平衡及前后/横移/转向按住控制，松开/失焦归零、250ms失联归零。模型数值核对、前后端测试、20只读快照及资源哈希通过；共享物理标定/舵机寄存器保持，未执行实物动作，见[舵机记录](03-舵机实测.md)与[平台记录](04-网络与观测平台.md)。

2026-10-06 v16总2万轮完成：大步/抬脚形成，但0.1命令实际约0.22m/s、零指令及停步失稳，仿真未通过；视频已保存，未部署实机，见[舵机记录](03-舵机实测.md)。

2026-10-05用户将v16扩至总20000轮，保留100轮进度；每500轮保存/1000轮评估，长训练中间只记录选模，未正常步态验收/无实物部署，见[舵机记录](03-舵机实测.md)。

2026-10-05 v14因步幅退化在400轮检查停训；v15短训无抬脚改善，已启动v16摆腿奖励占比100轮对照，仍未正常步态验收/无实物部署，见[舵机记录](03-舵机实测.md)。

2026-10-05 v14修复100轮仿真步幅21→30mm、偏航33→17度，抬脚/腾空仍不足；已续至总1000轮并每100轮退化检查，未验收/无实物部署，见[舵机记录](03-舵机实测.md)。

2026-10-05修复旧预检failed结束状态阻止再次归零，并显示具体原因；全前端51测试/构建及静态部署核对通过，旧标定225保持，未归零或发动作。当前相对roll约−16.1°尚未确认实际倾角，见[IMU记录](02-IMU实测.md)。

2026-10-05 v13诊断：250轮已有步态退化；瞬时走直门控将v12迈步奖励压至平均约12%、完整步幅得分近0。导出数值对齐，建议分阶段修奖励/中间验收，未新训或实机动作，见[舵机记录](03-舵机实测.md)。

2026-10-05按用户反馈完整撤销新归零，旧IMU参考与安装字段整份恢复至225（仅bootId适配），关节/位置保持；此前224只恢复部分字段，显示效果待核对，见[IMU记录](02-IMU实测.md)。

2026-10-05修复归零时整体替换IMU段导致安装字段遗漏：恢复原方向/目标，用户归零参考及位置/关节保持；20相关测试和部署资源核对通过，需刷新前端，未启动动作，见[IMU记录](02-IMU实测.md)。

2026-10-05 v13完成1000轮但0.1m/s起步失败：仿真32/32跌倒，无有效迈步，未通过；已停训/未部署实机，见[舵机记录](03-舵机实测.md)。

2026-10-05修复Pitch/Roll归零按钮在手动指令完成commanded状态下误禁用，前端已更新；未改当前标定，见[IMU记录](02-IMU实测.md)。

2026-10-05位置标定旁新增Pitch/Roll归零按钮：当前姿态设为0°、保留Yaw，保存共享IMU参考；12项相关测试/前端构建及资源核对通过，未代用户点击或发动作，见[IMU记录](02-IMU实测.md)。

2026-10-05按用户要求把主板资源读数移到重启/关机同一行中间，前端更新及资源核对通过；后端未重启，见[平台记录](04-网络与观测平台.md)。

2026-10-05主板资源状态栏上线：Rust独立线程每2秒采集SoC温度、CPU平均利用率、内存已用/总量及百分比；实测采样约2.001–2.002秒，56测试/ARM/前端构建通过，标定与舵机寄存器保持，未发动作。见[平台记录](04-网络与观测平台.md)。

2026-10-05 xgoduck-schedule-v2上线：模型50Hz推理、100Hz写最近目标→读反馈，短读6ms/完整配置30ms。54测试通过；移除实测阻塞约12ms的Linux tcdrain后，只读交错1000请求0缺/0坏校验、约100Hz，20部署快照完整/标定寄存器保持。未启动动作，运动/站立效果待用户手测，见[舵机记录](03-舵机实测.md)/[HAT记录](07-HAT实测.md)。

2026-10-05按用户要求启动v13 0.1m/s专门优化训练1000轮（v12暖启动），强调完整迈步/走直/起停；短测试通过，未验收/无实物部署，见[舵机记录](03-舵机实测.md)。

2026-10-05 v12补充0.05m/s仿真回放：实际约0.031m/s、同脚步幅14mm，5/32环境跌倒，慢速仍不稳定；无实物动作，见[舵机记录](03-舵机实测.md)。

2026-10-05 v12已完成1000轮：仿真开始迈步、头姿改善，但同脚步幅36.8mm、明显走偏，未通过正常步伐验收；已停训，无实机部署，见[舵机记录](03-舵机实测.md)。

2026-10-05用户手测并发站立仍不稳：整板已重启/API只余启动日志，故障文件与上一轮助手测试完全相同，不能作本次证据。当前idle/15失能，未触发新动作；需保留下一次启动至失稳的连续数据并核对异步延迟，见[舵机记录](03-舵机实测.md)。

2026-10-05按用户要求提高真正行走得分并启动v12新1000轮（4096环境），完成后评估/视频并停止；未部署实机，见[舵机记录](03-舵机实测.md)。

2026-10-05 async-inference-v1上线：独立推理线程/最新单槽，UART单线程串行，53测试通过。用户托稳授权重启验证约3秒后#11越界自动卸力：167读取/41不完整/98校验错，读平均11.63ms、写0.22ms，反馈55.92Hz/发令30.06Hz，未完成15秒；卸力后只读97.52Hz。尚未达运动100/50Hz，见[舵机记录](03-舵机实测.md)。

2026-10-05 v11已完成1000轮及评估：头姿改善但前进近乎原地站立，仿真未通过；视频已保存，未续训/未部署实机，见[舵机记录](03-舵机实测.md)。

2026-10-05用户开启模型后只读100不同反馈快照：运行50Hz发令/累计最大间隔30.68ms，抽样0缺失/0校验错误、14实读P6/D20使能；采样不覆盖每帧，不可当完整0丢包。见[舵机记录](03-舵机实测.md)。

2026-10-05 split-gains-v1上线：用户指定姿势/手动/HOME32/40，到位后模型14关节切6/20并回读确认；51测试通过、20只读快照完整、物理标定及寄存器保持。未启动动作，抽动/反馈损坏仍待实测。见[舵机记录](03-舵机实测.md)。

2026-10-05用户报告P32固定姿势到位但模型抽动：匹配策略故障实际17.5Hz/最大182.6ms、停机前三帧校验错误2/4/2及大量缺失，多关节目标饱和，最终#10范围超出卸力。实际硬件32/40，策略统计旧6/20标签不可作实参证据；增益/模型与通信贡献待分离，本轮只读。见[舵机记录](03-舵机实测.md)。

2026-10-05用户要求恢复P32/I0/D40，restore-gains-v1已上线，主动控制/快读缓存条件同步；50测试通过、20只读快照完整、标定和寄存器保持，下一次控制临时生效，无动作验收。见[舵机记录](03-舵机实测.md)。

2026-10-05新故障记录149帧：最终74帧14颗内部目标均匹配发令，#12稳定+57步/#30−140步，仍使能/非零电流/fault0；证明本次目标收到但位置没跟上，参数/重复写固件影响及机械条件待分离，无新动作，见[舵机记录](03-舵机实测.md)。

2026-10-05同条件#30静态站姿超时−12.57°：旧策略故障文件不对应本次，尚无根因证据。已上线pose-evidence-v1，保存卸力前完整帧/内部目标/前一命令，50测试通过、20只读快照完整、标定及寄存器保持；无新动作，待同姿势复现，见[舵机记录](03-舵机实测.md)。

2026-10-05用户反馈行走低头/小碎步，已启动v11正常步伐新训练（4096环境/先1000轮，匹配EMA；用户决定是否续训）；短测试及导出检查通过，尚未完成步态验收，未替换平台模型或执行实物动作，见[舵机记录](03-舵机实测.md)。

2026-10-05用户托稳#13真实单关节P6/P12/P6对照完成：±3°与同HOME1954均可达，HOME误差1.23→0.62→1.23°，两组2250发令约49.991Hz/0缺失；轻载电流约16与昨日承重失败171不同，整机负载下P12效果未验。回中心/15卸力，原SRAM与标定恢复，平台active，见[舵机记录](03-舵机实测.md)。

2026-10-04 #13 HOME超时深入核对：后续同类故障原始卸力前两帧目标1954/实际2022（5.98°）、速度0/非零电流，目标已收到，策略未运行；静止受力/跟随偏差待隔离验证P6/I0与机械阻力贡献，无动作/改参，见[舵机记录](03-舵机实测.md)。

2026-10-04用户反馈开始模型站立轻推响应慢倒地；后台重启丢失当时策略记录，只读后续30份完整/空闲97Hz，不能代表失败时控制。启动HOME线性分段不作用于策略loop；EMA/执行器响应与v5抗扰能力待分开核实，无动作或改参数，见[舵机分析](03-舵机实测.md)。

2026-10-04 linear-pose-v1已上线：手动姿势/模型HOME为1.5秒50Hz匀速直线分段，无smoothstep/EMA；仅模型推理EMA0.45。49测试及构建通过，20上线快照完整，标定/寄存器保持，无实物动作验收，见[舵机记录](03-舵机实测.md)。

2026-10-04 direct-pose-v2补充诊断已上线：超时附卸力前ID/误差/反馈保持和到位计时；20快照完整，两模型就绪，标定/寄存器保持，无动作。原不到位仍待下一次手动尝试定位，见[舵机记录](03-舵机实测.md)。

2026-10-04用户反馈直接姿势仍3秒超时且卸力后才变姿势；日志同时有后续一次到位成功，卸力后误差不能判定原失败关节。已补卸力前ID/误差/连续确认诊断并构建通过，direct-pose-v2因主板SSH再次不可达尚未上线，见[舵机记录](03-舵机实测.md)。

2026-10-04 direct-pose-v1已上线：按用户要求手动站姿/模型HOME取消2.2秒平滑、直接最终目标，仅模型推理EMA0.45；3秒真实到位确认保留。49测试与构建通过、20快照完整，标定/寄存器保持；未发动作，原不到位效果待实物验证，见[舵机记录](03-舵机实测.md)。

2026-10-04 model-walk-v1已上线：实机模型可选站立/行走，开始行走及模型行走姿势/前进速度就绪；原生数值核对差1.79e-7，20快照完整，在线资源与构建一致，标定/15颗舵机寄存器保持。无动作或实机行走验收，见[平台上线记录](04-网络与观测平台.md)。

2026-10-04行走模型平台接入本地完成：实机模型切换站立/行走、开始行走/模型行走姿势/前进速度，49后端测试与前端/ARM构建通过，本地浏览器核对。原主板192.168.31.186 SSH/HTTP超时，尚未部署或动作；待恢复连接上线，见[平台记录](04-网络与观测平台.md)。

2026-10-04 v6行走四组CPU名义仿真各20秒未倒地；BAM kp6+EMA0.45最大倾角4.82°、偏航P95 5.69°，但横向偏离0.55m/颈部HOME偏差−60°仍未验收。已生成视频，无实物动作，仿真增益非硬件PID映射；见[舵机记录](03-舵机实测.md)。

2026-10-04 20:54当前姿态只读对照：roll+0.17°/pitch−1.98°，约4秒14关节实测变化0°；shadow首帧原始目标最大8.16°、EMA后编码目标最大4.39°，实际发令0。硬件仍P32/D40与电流/速度500，新控制参数尚未动作验证；见[舵机记录](03-舵机实测.md)。

2026-10-04用户授权XgoDuck方案已部署xgoduck-v3：目标位置+电流0/速度0六字节，临时P6/D20（嘴P10）、EMA0.45、寄存器速度滤波、模型50Hz/反馈目标100Hz；本机标定保持。48后端测试及构建通过，上线100快照完整/实际95–96Hz，只读shadow0发令。未主动写硬件，参数下次控制生效；训练配置已同步但未重训/未CUDA验收，见[最新控制记录](03-舵机实测.md)。

2026-10-04对照XgoDuck：相同1910模型参数/训练kp5，对方运行临时P6/D20与EMA0.45，我们P32/D40且无EMA；名义仿真32+EMA恢复稳定但不能当实物PID映射。提出控制变量对照/模型特定配置，未改实物；底层42六字节及缺单颗年龄策略不照搬，见[完整对照](附件/HAT/2026-10-04/xgoduck-review/comparison.md)。

2026-10-04同部署v5的CPU/BAM HOME闭环20秒能收敛：kp5最大倾角1.70°、末5秒关节幅0.36°；编码量化/差分反馈也稳定。合成增益32失稳，提示执行器响应匹配需优先核实，但不能将HD原始P=32等同BAM32或直接改PID；v5历史整体仿真验收仍未通过。无实物动作，见[舵机分析](03-舵机实测.md)。

2026-10-04用户点击模型HOME报告站住：5秒40快照14位置不变/0缺帧，头颈19.95°/20.21°、最大HOME偏差1.25°、机身roll0.65°/pitch−0.33°；3次只读推理仍要求双踝约+10.55°/−9.4°，发令0/硬件及标定保持。不是当前头颈0°导致，连续模型闭环仍待验证，见[舵机记录](03-舵机实测.md)。

2026-10-04“模型站立姿势”按钮已部署model-home-v2，与开始模型站立相邻；同一模型HOME过渡后保持，不推理/不依赖IMU。45后端测试及前端构建通过，20上线快照完整/寄存器和实际标定保持，未点击有效动作；见[舵机记录](03-舵机实测.md)。

2026-10-04只读模型契约/HOME对照：关节顺序/61维/动作HOME+delta基本一致；当前静态头颈0°与训练HOME约20°差异明显影响腿部输出，理想HOME首帧踝仍约9–12°。部署SHA一致、未发动作/未改标定；真实启动已有HOME过渡，仍需匹配仿真闭环及实物HOME输入对照，不据shadow认定根因。用户确认IMU显示方向一致，见[HAT分析](07-HAT实测.md)。

2026-10-04用户确认实物躯干竖直后共享IMU参考183→184：roll/pitch约0°，关节零点/安装轴/硬件目标保持；只读shadow仍要髋/踝7–13°、头颈18–24°，说明参考偏差不是全部原因。固定姿态继续稳定、未运行模型；用户随后确认显示偏转方向一致；继续核对模型HOME与输入契约，见[IMU记录](02-IMU实测.md)/[HAT排查](07-HAT实测.md)。

2026-10-04用户托稳静态站姿观察：IMU恢复streaming/revision183；15秒224快照无缺失/坏校验，14目标和位置固定、最大误差0.703°，用户反馈基本不动。3次只读shadow仍要求腿约9–11°、头颈23–24°，硬件目标保持；模型body roll约−6.28°/pitch−5.27°，需用户独立确认实物竖直后区分真倾斜/参考偏差，尚未改标定或发模型动作，见[HAT记录](07-HAT实测.md)。

2026-10-04servo-io-cycle-v1已上线：连续已使能关节预检使用新鲜短反馈+最多1秒配置缓存，批量0/500按地址同步写，单颗回读不延迟全组低频刷新；保留50Hz顺序读写/Skip/故障与限位。45测试通过/1跳过，部署20及随后300只读快照完整无坏校验，标定/模型/前端/寄存器保持；#34部署前已使能并保持，额外动作回归因非全失能预检停止，未发新动作。见[HAT记录](07-HAT实测.md)。

2026-10-04不依赖IMU的14关节50Hz真实±1°同时动作三轮完成：1500组/21000关节目标，实际49.9986Hz/最大间隔24.188ms，22500反馈0缺失/0坏校验/oe无新增；14均实测位移1.05–2.02°峰峰。中心恢复最大误差0.4395°后全15卸力，service active，标定及目标电流/PID保持；原生I/O/调度复用，未跑模型/网页，未触发实物掉包兜底，见[HAT记录](07-HAT实测.md)。

2026-10-04用户托稳续验原生50Hz读15/写#23真实±1°三轮各500帧：实际49.9989Hz、最大间隔24.893ms，22500条反馈0缺失/0坏校验/oe无新增；位置2111–2132步有动作。复用生产原生Bus/Clock/Coast，1471快读+29配置刷新，未运行模型；中心恢复/全15卸力/service active/标定SHA保持。IMU仍unavailable，不能泛化为14关节或模型闭环稳定，见[HAT记录](07-HAT实测.md)。

2026-10-04用户托稳后新版angle接口#23真实±1°三轮/30请求完成：30/30反馈确认、201去重快照无缺失/坏校验，位置2109–2132步证实真实往返；结束恢复中心目标/全15卸力。实际请求5.259Hz，不冒称50Hz策略验收；当前IMU unavailable阻止模型验证，标定实读revision180。见[HAT记录](07-HAT实测.md)。

2026-10-04单关节短反馈确认已部署servo-angle-feedback-v1：写目标后HD0x82/15字节快读全15，选中ID最多3次/150ms内启动确认；缺帧保留已发送目标和使能并明确提示未确认，不重复目标/使能或冒称到位；有效故障仍卸力。后端42通过/1数值fixture测试跳过、前端44通过/build成功；模拟候选检查与实机20只读快照完整，标定/模型/舵机寄存器保持，无动作验收。见[HAT记录](07-HAT实测.md)。

2026-10-04复核官方HEAD9136aa4源码：Dynamixel2.0 0x8A合并快读、同步写、3帧coast/持续盲区保持/Skip周期；本平台已有策略容错，单关节angle仍完整回读/即时缺失失败，可优化目标确认路径但尚未修改。源码审查非实机，不能直接套HD0x8A，见[HAT记录](07-HAT实测.md)。

2026-10-04#23写固定当前位置对照完成：无额外flush/约50Hz，各模式1500请求；写后快读0缺、写后完整读1坏校验缺21/22两条，23反馈全正常。固定位置2120无位移，但首段仅23使能0→1（未发送使能，原因待查），结束单颗卸力恢复0/service active。单关节angle确认实际用31字节，不能泛称所有控制快读；见[HAT记录](07-HAT实测.md)。

2026-10-04深入15/31字节交错各30000请求完成：fast缺7/450000（0.001556%）/3不完整/1坏校验/oe+3；full缺60/450000（0.013333%）/25不完整/23坏校验/oe+28。两种最长连续缺帧1、fe/BREAK均0新增，全部异常无迟到残留；短读观察缺失率约低88%。实际全15卸力，配置保持/无动作，服务恢复后100快照完整，见[HAT记录](07-HAT实测.md)。

2026-10-04用户确认未改接线后链路失效：UART2 3秒fe+967/BREAK+674/oe+0，1Mbps8N1参数正常且仅平台占用；独立预检同样全缺，深入模式对照无法开始。持续接收帧/BREAK异常需定位输入信号/收发电路，不能继续归因网页或普通丢包，见[HAT记录](07-HAT实测.md)。

2026-10-04深入15/31字节对照未启动：当前平台全缺、独立5次完整预检也未收齐，脚本停止并恢复服务，无动作/配置写；需确认当前接线/供电变化，不把无有效通信当对照丢包率，见[HAT记录](07-HAT实测.md)。

2026-10-04同接线交错只读15/31字节各3000请求：fast0/45000缺/0坏校验，full缺10/45000/4坏校验/UART oe+4；平台恢复后300不同快照全完整。14使能/嘴失能保持、无动作或配置修改；高异常暂未复现，不能归因快读或宣布根治，见[HAT记录](07-HAT实测.md)。

2026-10-04平台在线/无应答跳变：300不同快照132不完整/120坏校验（快照占比，非条数丢包率），后台有真实损坏、UI按单帧online切换。当前仅observer持UART，56℃；累计fe1586两次不变，无当前过热/双占证据，运行与独立长读条件不同，根因待同接线对照。只读未改代码/动作，见[HAT记录](07-HAT实测.md)。

2026-10-04短接TH1只读复测第7轮自动停止：6完整轮18000请求缺37条/15坏校验、UART oe+16；触发#22使能128但求和校验通过（40与42低字节均0x80），随后恢复0，符合多字节损坏漏校验特征。平台恢复/15全卸力，无动作/配置写，未完成20轮，见[HAT记录](07-HAT实测.md)。

2026-10-04电源接入位置互换只读试验按用户要求结束：16完整轮/48000请求，35不完整/缺82条/34坏校验、UART oe+40；部分轮另保留，平台服务恢复，无动作/配置写，见[HAT记录](07-HAT实测.md)。

2026-10-04飞线R32只读复测按用户要求停止：仅5轮完成，15000请求/12不完整/缺26条/11坏校验，第6轮部分另保留；service已恢复、测试退出，无动作或配置写，未完成20轮，见[HAT记录](07-HAT实测.md)。

2026-10-04换另一块HAT后20×3000只读完成（轮内关闭GC）：43不完整/60000（0.071667%）、缺95/900000（0.010556%）、42坏校验、UART oe+45/fe+0；15个ID均曾缺，异常后无迟到残留、全偏移无解析漏有效包。新板仍复现字节损坏/溢出，未解决；服务恢复/全15卸力/配置保持，无动作，见[HAT记录](07-HAT实测.md)。

2026-10-04USB假超时定位：追加时间跟踪直接捕获Python generation1 GC停顿29.536ms跨过30ms期限，随后尾包全齐；轮内关闭GC对照3000帧/45000包0缺失0坏校验、最大间隔22.489ms。证明至少一例迟到由测试脚本引起，不归因转接板；不能解释原生Rust平台/HAT坏校验。独立只读，无平台接入或动作，见[HAT记录](07-HAT实测.md)。

2026-10-04官方USB转接板独立20×3000只读完成：900000期望回包，截止时8帧缺56条，但8帧原始字节均在下一请求前拼回555字节/15有效包，全部为迟到，观察最终缺失0/坏校验0；较HAT 66不完整/54乱码明显改善，未验运动。平台未接入USB、仍ttyS2，服务恢复，无动作/寄存器或标定写入，见[HAT记录](07-HAT实测.md)。

2026-10-04用户绕过HAT改接主板UART2后平台15全offline/rx0；ttyS2/uart2-m0已启用且非串口console，需确认半双工转换和实际接线，尚不能进行有效丢包对照。只读未改配置/发动作，见[HAT记录](07-HAT实测.md)。

2026-10-04全15同步完整状态读取20×3000完成：60000组缺115/900000条（0.01278%）、66组不完整（0.11%）；#24=21/#11=15/#32=13最多，15个ID均曾缺。54乱码组、UART oe+46/fe+0，12无校验组仅32/33/34缺16条，全偏移扫描无解析连带丢好包。服务恢复/15在线全卸力/寄存器保持，无动作，详见[HAT记录](07-HAT实测.md)。

2026-10-04状态/位置/电压/电流只读对照：标准Dynamixel1.0 READ缺1/22500（#34单字节ff，0.00444%），整组仅10.8–15.2Hz；HD0x82同步0/22500缺/坏校验，整组约50Hz，UART均无fe/oe新增。全卸力无动作，服务恢复/15在线/寄存器保持，不代表运动问题根治，见[HAT记录](07-HAT实测.md)。

2026-10-04Dynamixel只读探测：1.0 PING三轮15颗全回应45/45，2.0 PING45次均0字节；只证实1.0 PING帧互通，不冒称完整控制表兼容。observer已恢复/15在线全卸力/寄存器保持，未发动作，见[HAT记录](07-HAT实测.md)。

2026-10-03标准站姿v5模型3次只读推理仍要求髋约13°/踝8–13°/头颈19–25°，无动作、原目标保持；模型HOME头颈20°与静态站姿0°不同，且实机通信时序/动力学尚未匹配训练，不能把站直当策略固定点，见[舵机记录](03-舵机实测.md)。

2026-10-03用户明确脚掌重标定后共享踝零点revision177：当前撑地位置2396/1672映射为标准站姿踝角，14参考2100.7003/24参考1967.2997，控制与3D共用，无独立显示偏置。IMU/其他关节保持，无硬件动作，网页视觉待验，见[舵机记录](03-舵机实测.md)。

2026-10-03用户确认当前标准站姿后IMU位置参考重标定revision176：前端俯仰2.443°→约0°（10份最大绝对0.042°），roll/yaw同样归零；关节零点/安装轴保留，无舵机动作，见[IMU记录](02-IMU实测.md)。

2026-10-03连续双踝调整已将托稳站立前倾降至0.5°，20份全在线/fault0；当前#14目标2400/#24目标1673保持，标定未改。原站姿模板仍待持久更新、松手稳定未验，见[HAT记录](07-HAT实测.md)。

2026-10-03用户确认反向回正并授权继续：#14减1°/#24加1°实际下发且保留，前倾均值11.71→11.16°，无通信异常，14使能/嘴失能。标定未改、未持久修改站姿模板，见[HAT记录](07-HAT实测.md)。

2026-10-03已托稳双踝±1°实际动作对照完成：正向前倾增加，反向变化与支撑波动重叠，未认定有效纠正；原目标恢复，14使能/嘴失能，标定未改，无通信异常，见[HAT记录](07-HAT实测.md)。

2026-10-03当前站立前倾只读确认：20份pitch约10.3°，原始加速度按既有安装轴独立支持前倾约9.3°；关节距原固定站姿目标最大0.967°，零点/目标未变、非闭环平衡。真实站姿/装配受力匹配仍待修正，未改标定/发动作，见[IMU记录](02-IMU实测.md)。

2026-10-03未改标定但站姿前倾核查：关节零点/站姿目标/IMU参考与既有证据相同，到目标最大0.967°；文件SHA变化由前端自动确认新IMU会话导致，不能归因用户改标定。当前相对IMU倾角88.68°，采样时实物摆放状态待确认，无动作/标定修改，见[IMU记录](02-IMU实测.md)。

2026-10-03 #32限位换算修复上线valid-turn-v2：旧nearest将3656错误绕到-440后被硬件限位拒绝，改为仅选编码/实读硬件限位内的等价圈数。39测试通过，10只读快照完整；部署保留当前标定/寄存器，未发动作。底层乱码未根治，见[HAT记录](07-HAT实测.md)。

2026-10-03恢复反馈时间连续性修复上线history-recovery-v2：约266ms采样空档恢复时按真实间隔清模型历史，避免暂停不足200ms而撞250ms门槛卸力；38测试通过/20只读快照完整。首次标定变更校验失败已回滚，重试保留当前标定成功；14使能/嘴失能保持，无新增动作，底层乱码及动态验收仍待查，见[HAT记录](07-HAT实测.md)。

2026-10-03反馈过期兜底修复上线servo-feedback-age：真实舵机时间戳在observe越150ms时转暂停保持，其他故障照常；37测试通过，20只读快照完整，部署前后14使能/嘴失能及寄存器/标定/模型保持。无新增动作，动态兜底未验、底层乱码仍待查，见[HAT记录](07-HAT实测.md)。

2026-10-03策略再次反馈过期：第三次连续全缺帧时coast仍放行，但每颗真实时间戳在observe检查超过150ms而卸力，缓存年龄起点/检查时刻不一致造成保持兜底遗漏。59字节无有效包、校验0不等于无乱码；仅读取诊断无新动作，修复验证待完成，见[HAT记录](07-HAT实测.md)。

2026-10-03官方方式优化已上线servo-official-io：高频56–70/15字节、每秒完整配置刷新，回包555→315字节；跳过过期周期，运动3帧完整旧反馈容忍/第4帧暂停保持，真实fault/电压故障仍卸力。Rust36通过；三轮只读3000份2999完整/1乱码且oe+1，快读均值4.4ms，#32本窗口无缺但底层未根治。15在线，部署前后14使能/嘴卸力、目标/PID/电流/标定一致，无新增动作；实机动态兜底未验，见[HAT记录](07-HAT实测.md)及[当前规则](03-舵机实测.md)。

2026-10-03接收重同步修复上线servo-rx-resync：坏校验逐字节重找头，真实坏帧全分片回放/Rust30/Python50通过，标定及前端模型SHA保持。线上WS1000份985完整/2坏校验，原始有效包均保住但底层丢包仍在；100ms无清缓冲对照#32六次仍无回复，另捕捉主机约24ms处理间隔后读出10包完整。最终15在线全卸力，见[HAT记录](07-HAT实测.md)。

2026-10-03 SSH恢复后行走v10已启动500轮：保留25%头部输出的短模型低头17°/stride79mm，但1/8失稳、航向偏65°，未验收；16测试通过，独立任务每100轮检查，正式结果待查。未部署/动作，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-03托稳动作数量对照15轮4252帧：单腿/单头/4头/14关节各1乱码轮，4腿无乱码但2漏包轮；单颗动作也会损坏其他关节回包。4乱码轮3次oe+1，14关节#24完整37字节坏校验但oe不增，不能归因唯一溢出原因。实际位置反馈证实动作，平均约50Hz/max60.08ms；全部卸力确认、最终15在线全torque0，见[HAT记录](07-HAT实测.md)。

2026-10-03分组只读17轮6636帧：全卸力/位置命令0，#12/#23各1500完整；#32单读及4/15颗组偶发整包未收、无oe新增；排除#32读14颗仍两次字节损坏且oe各+1，另一次74字节截断。动作不是乱码必要条件，少量也非绝不缺反馈；最终15在线全卸力，动作组待当前托稳确认，见[HAT记录](07-HAT实测.md)。

2026-10-03十轮往返统计：2/10轮故障，4598观察帧缺2（0.0435%），68970条反馈解析缺6（0.00870%）；本批均31/32/33、一次33原始有效。故障轮oe各+1，轮间也有新增；平均50Hz但最大间隔69ms，提前终止样本非完整100秒概率。15在线全卸力，见[HAT记录](07-HAT实测.md)。

2026-10-03托稳3轮往返：1轮500帧完整，另两轮首异常缺23/24/30及24/31/32，#30/#32原始有效却被解析连带丢失；各异常轮UART oe新增1、完整轮无新增，缺失ID不固定。当前15在线全失能，未改DMA/解析器，见[HAT记录](07-HAT实测.md)。

2026-10-03小幅故障细分：#11头插1字节、#12缺10字节，#13完整包被坏包解析吞掉；546=555+1−10。UART缺dma-names回退中断、IRQ实际CPU0，加上oe新增支持接收溢出重点；信号/供电仍待排除，仅分析未改线/代码，见[HAT记录](07-HAT实测.md)。

2026-10-03托稳14关节±1°50Hz往返复现：181次/49.998Hz、3.6秒后缺11/12/13，546bytes/校验1/乱码；UART2 oe12→13、fe不变，有效电压7.4–7.8V。已停止并15在线全失能，不重试；优先查接收溢出及信号，见[HAT记录](07-HAT实测.md)。

2026-10-03 roll校正后第一组策略目标回包全损坏：此前两帧555bytes完整，故障323bytes/校验3/全15无有效包；卸力后10/10全在线全失能。动作瞬变/信号/收发根因待查，TTL TXD是否仍连接待确认，未重试，见[HAT记录](07-HAT实测.md)。

2026-10-03用户确认当前无侧倾，仅roll校正保存revision155：4.79°→约0°（10份最大绝对0.008°），pitch约6.32°/yaw保留，关节零点/安装矩阵不变，无动作；重启重复性未验，见[IMU记录](02-IMU实测.md)。

2026-10-03行走v9完整500轮仍失败：最终七场景32/32失败/重置，前进低头82°；中间100/200无失败但也低头70°，未采用。头部HOME干预可移动但转弯严重；独立头部重初始化短模型低头降到3°但1/8失稳，正在验证渐进初始化，尚未合格或部署，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-03行走v9仿真：查实旧训练未裁剪最终目标、与实机官方范围裁剪不一致；已对齐并加入终止成本、修正续训学习率。14测试/短训练/8环境20秒无重置通过，但颈部仍低头59°，正式500轮及中间检查点对照正在运行，未验收/部署或动作，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-03实时3D贴地修复上线186:8877，release20261003-model-ground：按双脚实际顶点最低Z定位模型，保留相对抬脚高度；44测试/构建及线上资源核验通过。仅显示约定、无实物标定/动作/后端重启，视觉待刷新核对，见[平台记录](04-网络与观测平台.md)。

2026-10-03在线/无应答跳动核查：前端按最新后端online字段逐帧显示，此前确有孤立缺行；本轮HTTP100及网页同订阅WS400/400均完整、无断连，当前未复现。未改显示/缓存旧数据，间歇根因待查，见[平台记录](04-网络与观测平台.md)。

2026-10-03重启后实物/网页均倾斜：零点/IMU参考/安装矩阵与部署前一致，普通stand保持而非闭环；有效关节目标残差≤0.79°，计算倾角约13.6°，参考重力与安装Z轴差约6.6°。参考一致性及重启前后机械/融合对照待查，未改标定/发动作，见[IMU记录](02-IMU实测.md)。

2026-10-03关机/重启按钮已部署186:8877，release=20261003-power-controls；确认后systemd延迟5秒正常处理，旧版本/标定保留。Rust28/前端43测试及构建通过，线上请求拒绝/受限socket/资源字节核验通过；实际关机重启周期与视觉未验，见[平台记录](04-网络与观测平台.md)。

2026-10-03核查v8：500轮及验收结束，低头仍严重（前進neck_pitch平均−75°），所有场景32/32低躯干重置，stride退到45mm，优化失败。已停训、保存证据，未替换本机v6或部署实物，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-03 TTL修复启动完成：手动进入Linux后备份配置/标定、重建armbianEnv普通文件，内容SHA256保持一致；两次自动启动均正确加载配置/DTB/overlay，正常启动后186:8877 health成功。当前IMU/舵机/ToF无通信、物理连接未确认，启动故障根因仍未定，见[主板与接线](01-主板与接线.md)。

2026-10-03 COM5启动实测：DDR/SPL完成并进入U-Boot；mmc1:1启动时armbianEnv读取越界、板型DTB加载失败，最终无有效设备树并停在`=>`，未进入Linux。优先排查SD卡/文件系统/启动文件，根因未定，未重刷，见[主板与接线](01-主板与接线.md)。

2026-10-03 v8修正：v7已定位为low_trunk重置、检查点与导出一致；从v6恢复并改为头姿/行走联合正奖励，10项回归/短训练/导出及8环境20秒零termination检查通过，低头仍未验收。GPU7追加500轮正在运行，未部署实物，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-03用户反馈主板无法启动：尚无启动日志或当前电压，根因待查；已提供移除HAT后USB-TTL仅GND/RX读取UART2早期日志的方案，未接线实测，见[主板与接线](01-主板与接线.md)。此前启动成功不代表当前正常。

2026-10-03核查v7头姿500轮：训练与验收已结束，低头仍严重（前進neck_pitch平均−74°）；全部场景32/32启动后出现失败/重置，类型待诊断。整体未合格、当前停训，未替换本机v6或部署实物，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-03行走低头：v6前进仿真颈部相对HOME平均−62°，旧停步检查未覆盖行走阶段。v7加强持续头姿偏差训练，9项回归/短训练/导出检查通过，GPU7追加500轮正在运行；保留步幅与直线轨迹验收，效果待验证，未部署实物，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-03核查行走v6：追加500轮与仿真验收已结束，前进航向偏差P95112→45°、横偏1.67→0.68m，改善但仍不直；同脚stride78→70mm退步。前后行走无倒地，左原地转向6/32失败、头姿仍偏，整体未验收；当前停训、未部署行走，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-03策略首组目标后通信失败：301bytes、#20校验和111≠255、79乱码字节，双腿10颗缺失，头部5包有效；之前两帧555bytes完整。有效头部电压有下降但唯一根因未定，UART累计错误缺前值不能归因；卸力后15/15帧完整、15全失能，无策略重试，见[HAT记录](07-HAT实测.md)。

2026-10-03新IMU竖直参考下3次只读推理成功，倾角约0.055°、目标无饱和且重复稳定；髋俯仰/头颈修正较旧参考明显变小，最大仍右踝约10°，不能视为原HOME完全不动。未发动作/启动平衡，14使能嘴失能保持，见[舵机记录](03-舵机实测.md)。

2026-10-03用户明确按当前竖直HOME重存IMU参考成功revision150，15次只读倾角均值0.012°/最大0.019°；关节零点和安装矩阵不变，15在线、14使能嘴失能，无舵机动作/平衡启动。新参考方向/回正待复验，物理竖直依据用户确认，见[IMU记录](02-IMU实测.md)。

2026-10-03站稳HOME下3次只读v5推理成功，无饱和/无动作；IMU倾角约9.44°，模型仍拟明显调整，最大右髋俯仰约18.66°，三次目标跨度≤0.14°。14使能嘴失能保持；仅单步预测，不能证明闭环稳定，HOME躯干真实倾斜待核对，见[舵机记录](03-舵机实测.md)。

2026-10-03用户确认新标定后HOME“已经站稳了”；随后5次只读均15在线/fault0，14使能嘴失能。保持HOME，无策略启动；松手时长/抗推条件未说明，不当作动态平衡验收，见[HAT记录](07-HAT实测.md)。

2026-10-03新标定后用户托稳授权v5 HOME仅保持成功：2.4秒、117次发令49.99Hz/max20.66ms，14关节±5°持续100ms通过；后台恢复后15在线/fault0、14使能嘴失能。仅HOME到位保持，未启动平衡或验松手站稳，主控186，见[HAT记录](07-HAT实测.md)。

2026-10-03用户确认全部舵机方向与前端一致；基本软件零点记录及手动方向核对完成。新主控192.168.31.186:8877只读访问成功，同一deviceId、revision148、15在线全失能，15零点与昨日保存一致；带载动作/平衡未验，本次无写入，见[舵机记录](03-舵机实测.md)。

2026-10-02头颈嘴30–34仰卧软件零点保存成功revision147，旧参考最大差4.04°；双腿与IMU不变，15在线/全失能，无硬件中位或动作。全部15静态参考已记录，转向与机械绝对精度仍待验，见[舵机记录](03-舵机实测.md)。

2026-10-02右腿仰卧软件零点保存成功，revision146，10–14回读一致，旧参考最大差3.87°；IMU、左腿及头部参考不变，15在线全失能，无硬件中位或动作。双腿静态参考已更新，转向/机械精度及头部待验，见[舵机记录](03-舵机实测.md)。

2026-10-02左腿用户摆好仰卧零位后仅position采样/保存成功，revision145，20–24软件参考已回读；旧零点最大差3.32°。IMU与其他关节参考不变、15扭矩0，无硬件中位或动作；机械精度及转向待验，右腿待摆，见[舵机记录](03-舵机实测.md)。

2026-10-02关节零点重核准备：revision144旧零点和实时快照已保存，15在线且全失能；确认仰卧supine参考为全0，拟先对齐左腿并仅position采样。未覆盖IMU、未写硬件中位/关节零点或发动作，用户摆位待完成，见[舵机记录](03-舵机实测.md)。

2026-10-02前端IMU新增总倾角及前后/左右倾方向提示，沿用现有共享参考和安装矩阵；四组真实姿态计算对照、43测试/构建通过，线上7资源字节一致。未重启后端或改标定/发动作，15在线；浏览器视觉未验，见[平台记录](04-网络与观测平台.md)。

2026-10-02前倾/右倾后回正只读核对：同revision144，总倾角0.77°、俯仰+0.54°、横滚-0.55°，与首次竖直两轴差0.05°/0.18°，基本静态方向和本次回正重复性通过。20样本稳定、15在线，未改标定/发动作，不代表动态精度或实机平衡验收，见[IMU记录](02-IMU实测.md)。

2026-10-02右倾方向只读验证：同revision144，横滚+20.69°、俯仰-1.92°，主要变化在横滚且右倾符号正确；与前倾窗口结合支持两轴未互换。20样本稳定、15在线，无标定/动作写入，回正重复性及角度绝对精度待验，见[IMU记录](02-IMU实测.md)。

2026-10-02前倾方向只读验证：同revision144，俯仰由+0.49增至+21.11°，主要变化在俯仰且符号正确；横滚由-0.73至+2.92°，实际侧倾/轴串扰未区分。20样本稳定、15在线，无标定/动作写入，侧倾及回正待验，见[IMU记录](02-IMU实测.md)。

2026-10-02用户摆好竖直后20次只读：revision144下总倾角均值0.88°、俯仰+0.49°、横滚-0.73°，短窗口稳定；末次15颗在线，先前11颗失联恢复原因未知。未保存标定或发动作，动态方向及通信长期稳定性未验，见[IMU记录](02-IMU实测.md)。

2026-10-02用户确认没动线、全部SH2.5线端一起连接；追加PING/6字节身份/1字节扭矩读取均固定11颗零回包，正常4颗全部短包通过。非整段反馈长度或SYNC机制问题，线端信号/地路径与设备通信卡住仍待物理对照；无复位/断电或舵机写入，见[HAT记录](07-HAT实测.md)。

2026-10-02固定11颗缺失独立串口只读对照：停后台后100ms逐颗READ和正/倒序SYNC READ均仅30/32/33/34应答，其余0字节；UART2 fe/oe无新增，恢复后台仍相同。用户反馈仍使能、电源正常；优先核对共同DATA/地连接，具体线路/固件原因尚未定位，无舵机写入，见[HAT记录](07-HAT实测.md)。

2026-10-02调整IMU后再次请求只读推理被拒：#20反馈丢失；连续3快照仅30/32/33/34在线，缺10–14/20–24/31，回包148bytes、校验0。当前完整关节姿态不可得，未产出新预测、未发动作，缺失关节扭矩未知；见[HAT记录](07-HAT实测.md)。

2026-10-02用户调整IMU后10次只读复核：当前revision142下俯仰+5.37°、横滚+0.15°，模型约定仍观测前倾约5.4°，此前约10.6°；短窗口稳定。未写标定或发动作，真实机身倾斜与安装/参考偏差尚不能区分，见[IMU记录](02-IMU实测.md)。

2026-10-02 HOME保持下关节+IMU三次只读推理：相对标定倾角10.61–10.66°、角速度小；模型下一步多关节调整约10–19°，#21原始-24.83°裁至-22°。未发运动、14使能/嘴失能不变；仅独立单步预测，未来平衡/倾倒未验证，见[舵机记录](03-舵机实测.md)。

2026-10-02最新用户要求v5 HOME仅保持重试一次成功：2.4秒到位，117次发令49.99Hz/max20.42ms，14关节±5°持续100ms通过并保持使能，嘴失能，15在线/fault0；无平衡推理。仅托稳单次HOME验收，不覆盖历史回包损坏或证明自主站稳，见[HAT记录](07-HAT实测.md)。

2026-10-02最新v5 HOME仅保持试验：用户托稳授权，未运行平衡推理；过渡进度1.0/116次发令49.99Hz/max21.33ms后反馈缺21/22/23、551bytes/校验1，保持确认失败自动卸力。头颈约20.30°/18.98°接近HOME，但未保持成功；最新15在线全失能/fault0，平台已恢复，无自动重试，见[HAT记录](07-HAT实测.md)。

2026-10-02本次HOME故障动作幅度核对：保存启动前反馈至HOME计划#30约20.53°、#31约19.74°，其余12颗≤1.16°；失败前末条位置目标仅#30/31各偏4步，其余保持初始值，尚未进策略循环。#24计划仅0.29°，当帧校验失败无法确认位移，见[HAT记录](07-HAT实测.md)。

2026-10-02最新模型HOME过渡#24失败：真实触发帧555bytes但#24校验错误，14包有效、读取22.00ms；摘要误用了卸力后missing[]/校验0帧。发令49.87Hz/max20.33ms，未因频率触发；当前15在线全失能，运动通信包损坏及诊断混帧待修，见[HAT记录](07-HAT实测.md)。

2026-10-02最新#23不动根因已对照确认并修复：目标后紧接重复使能使内部目标保持旧值；单写位置约1°可动。已部署20261002-servo-write-order-fix，已使能不重复使能、未使能先对齐/使能回读再发目标；Rust27/Python33测试通过，平台HTTP实物约1°往返两次均动且内部目标同步，15在线、扭矩/标定未改变。只验#23已使能小幅动作，其他关节/未使能实物及整机未验收，见[舵机记录](03-舵机实测.md)。

2026-10-02最新#23十几次完全不动：15次请求逐1°累计，新请求寄存器2137但内部目标仍2308、实际2311，差约15°；4次只读一致，确认内部目标未更新，不能归因普通1°摩擦残差。写目标后重复使能与模式4固件处理待对照；未发动作/改参数，见[舵机记录](03-舵机实测.md)。

2026-10-02 #23再次连续点击：11次逐1°累计请求均有下发日志；复现过程位置未抓到，修正WS地址后60秒只读窗口无新请求，目标2308/实际2311稳定。暂未定位小步动作阈值，未改参数或发动作，见[舵机记录](03-舵机实测.md)。

2026-10-02最新#23速度/加速度核验：连续3次实际回读0/500，加速度0按资料为最大，速度500仍是最高速度参数；手动微调无2.2秒轨迹。未改参数或发动作，不能确认该速度参数是小步不响应根因，见[舵机记录](03-舵机实测.md)。

2026-10-02用户反馈累计数度才动作：日志已逐1°累计，实时#23存在约1°带输出残差；#32/33请求目标与内部目标反馈不一致且电流接近0，固件目标处理/重复使能与摩擦负载均待区分，不能直接归因死区。仅只读、未改参数或发动作，见[舵机记录](03-舵机实测.md)。

2026-10-02行走v6继续优化：保留v5大步幅目标，新增持续偏航成本与左右镜像损失，足端镜像物理核对/9项回归/短训练/归一化导出通过。GPU7已启动追加500轮，当前未完成验收；未改实机控制或部署行走，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-02最新±1°按钮故障：身份监听返回新数组，关节采样/同版本标定刷新误清手动目标，造成重复点击反复发送同一角度；已修复多源逐项比较并部署20261002-servo-nudge-fix，42项前端测试/构建及资源SHA256核验通过。后端未重启，15在线/14使能/嘴失能未改变；助手未发动作，实物效果待用户刷新验证，见[平台记录](04-网络与观测平台.md#2026-10-02±1重复点击目标被刷新清空修复部署)。

2026-10-02行走v5追加500轮已完成并停训：同条件32环境前进stride62.36→77.60mm、抬脚16.88→22.12mm，步幅扩大；航向偏差P9577.56→111.53°、横偏1.67m，直走退步，整体仿真未验收。所有七场景无倒地，头姿/横移转向仍不合格；未部署实物、未继续追加，见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-02扩大步幅/直走仿真：v4追加500轮完成，偏航明显改善但仍拐弯，同条件stride约64mm未继续扩大；航向偏差P95384→60°、横偏仍1.23m，未通过验收。已通过7项回归/短训练/归一化导出，GPU7启动v5追加500轮继续优化，当前未验收、未部署实物，见[舵机记录](03-舵机实测.md)及[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-02小步走仿真诊断：v2-500有效落脚约10.23次/秒、同脚stride中位33.93mm、腾空70ms、抬脚峰值3.68mm。新增完整摆腿落地质量奖励及步态指标，5项回归/短训练/归一化导出通过，GPU7的v3追加500轮已完成。同条件stride33.93→61.84mm、腾空70→140ms、抬脚3.68→15.28mm、落脚10.23→7.07次/秒；偏航/停步/头姿仍不合格且部分退步，未改变实机控制或部署，详见[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-02行走仿真：v1完整6000轮结束但验收失败，约68°身体撑地且几乎不跟随速度；检查点/ONNX基本一致。已修正仿真奖励与失败判定，HOME/3项回归/5轮短训练通过，GPU7启动v2后，用户改为先训总计500轮；已保留检查点续训并完成总计500轮（2048环境）。七场景无倒地/倾角P95约3.5–4.3°，前后速度改善，但偏航/头姿、横移转向仍未通过验收；已停训；未改变实机控制或部署行走，见[舵机记录](03-舵机实测.md)和[训练记录](../../../microduck-rl/docs/hd1910-stand-training.md)。

2026-10-02最新平台已切换原生Rust后端，release=20261002-rust-native-v2；前端51个文件不变，标定逐项相等、15颗在线/扭矩未变。26项Rust及HTTP/WS对比通过；正式20秒CPU单核口径153.28%→46.02%、舵机WS31.28→49.42Hz、IMU约49.7Hz，987/989观察帧完整，仍有少量缺行。只读、未发动作；ToF/相机旧故障仍未恢复，见[平台记录](04-网络与观测平台.md#2026-10-02-rust后端迁移与正式切换)及[验证报告](../validation/20261002-rust/README.md)。

2026-10-02最新rsyslog高CPU：tofd句柄1024/1024刷accept错误，47MB日志内存盘写满，rsyslog又刷写入失败；已临时停tofd、备份并释放syslog空间、恢复rsyslog。处理后10秒CPU0.5%、日志盘1%、平台active；ToF仍未恢复且开机可能再次启动，句柄泄漏根因待修复。见[网络与平台](04-网络与观测平台.md)及[ToF](06-ToF实测.md)。

2026-10-02最新舵机低频/掉线：主控85℃触发热降频、用户确认无风扇；UART2溢出计数91→115。已暂停失败重启的相机服务，取消漏包后跳读1秒并合并IMU积压发布；全15在线网页样本89/179→173/178，实际扫描约39Hz、网页约9Hz，仍未达50Hz。新release=20261002-servo-scan-recovery，剩余超时/散热未解决，详见[HAT实测](07-HAT实测.md)。

2026-10-02平台前端新增舵机±1°即时微调与选中舵机3D橙色高亮，已部署到192.168.31.193:8877，构建/41项软件测试通过；未发实物动作、未重启后端，浏览器视觉复核因工具超时未完成，详见[网络与观测平台](04-网络与观测平台.md#2026-10-02-舵机微调与3d选中联动)。

| 主题 | 当前已知结论 | 完整记录 |
|---|---|---|
| 主板与接线 | HAT供电及针序见07；10-02模型动作时舵机供电塌压；2S细线已确认，建议分路粗线供电/共地，未改线验收；已汇总MS901M四线：VCC/GND/TX/RX→Pin1/6/16/18；ToF SDA/SCL→27/28，供电与LPn待逐线核对；旧BNO085接线已标为历史 | [主板与接线](01-主板与接线.md) |
| IMU | 10-02竖直静止采样通过；位置标定删除安装矩阵已修复部署，用户确认后竖直参考保存revision128，当前倾角约0.09°、舵机标定不变、未发动作；动态轴向/通信仍待验，见07；10-02外接MS901M已迁到HAT J7/ttyS9，平台约197Hz、量程确认，I/O错误0，安装标定显示未完成，详见07； 10-01模型报过期后改为采集线程直接共享最新值，专项9项/5次真实只读通过、约199Hz；后续实机启动改报#21通信丢失，长期闭环未验证；10-01重力轴标定安装矩阵已保存revision81；前倾约87°、两轴正交与方向数值核对通过，舵机配置未变；用户确认屏幕方向正常，自主闭环未执行 | [IMU 实测](02-IMU实测.md) |
| 舵机 | 最新HAT模型首次发令缺8颗/乱码，有效电压6.7–7.2V，未见此前严重塌压；故障后15在线/全失能；模型收到17.34°但用户称竖直，参考待核对，见07； HAT供电/串口/改焊及最新通信统计见07；10-02用户模型首次发令后捕获7.3–7.8V跌到4.7–5.5V、9颗回包缺失/乱码；2S细引出供电线已确认，线路压降与电池能力待测；最新全15颗失能确认，供电恢复7.8–8.1V；此前10-01用户启动#21反馈丢失、当时10–14/21失能未确认；10-01站立v1虽小扰动无倒地，但用户反馈及CPU关节核对确认扭腿，站姿验收不合格；v2-250冻结目标对照确认有小幅主动纠偏，但静态目标也能承受小推，0.30m/s前后仍倒，抗推不合格；本机已补身体碰撞；v3完成3000轮，0.30连推无倒地但站姿偏移/持续运动，验收失败；已打开v3诊断预览；v4b完成2000轮，静止关节跨度p95约0.15°但头侧倾25.55°、验收失败；v5完成2000轮，静止头误差p95 1.17°，交替连推2/32倒地、验收失败，已打开v5诊断窗口；用户认可效果，v6追加3000轮完成但抗推退步；用户选择v5已部署平台站立闭环入口，仅完成真实只读推理、未发模型运动，原模型越界导致失能已改为官方角度边界饱和，只读通过，实机平衡未验证；新增HD1910行走独立任务，首轮6000轮训练中，未部署行走；全部15关节开放滑块，嘴部#34按用户确认范围0–30°，角度下方显示编码器位置，清理遗留速度/加速度为500/0。已部署，快照revision108、15颗在线、14使能/嘴部失能；助手未发动作。用户反馈嘴部不跟手；已实际修正#34加速度0/速度500并回读，目标未变，跟手效果待用户验证。实际动作日志47.25–50Hz，本轮未运动验证；模式4速度/电流语义仍待核对 | [舵机实测](03-舵机实测.md) |
| 网络与平台 | 最新平台15在线/全失能，模型首动作回包损坏与IMU参考不一致待处理，见07；以下为历史过程； HAT迁移配置、备份及验证见07；10-02发布policy-v5-bus-trace，24项专项通过并捕获供电塌压回包，首动作观测/姿态待核对；此前10-01 imu-mailbox发布已通过IMU共享通道只读，后续通信故障见10-02新证据；10-01远程训练服务器已部署，HD1910模型与replica几何已接入站立维持训练，v1的1000迭代小扰动平衡通过但腿部姿态不合格，v2-250小扰动姿态与平衡通过、大幅前后扰动倒地；本机补齐身体接地碰撞，v3的3000轮已结束，连推存活改善但静稳姿态与运动指标不合格，验收失败、诊断窗口已打开；v4b已完成但头部偏斜、验收失败；v5已完成，头部改善但交替连推2/32倒地、验收失败，本机v5诊断窗口已打开，用户认可效果，v6追加3000轮完成但失败，已按用户选择部署v5实机模型控制与只读检查；模型越界失能已纠正，按官方边界饱和后只读通过，未执行模型运动；新HD1910行走首轮6000轮训练中；Windows本机六向扰动可调，0.30m/s前后右扰动倒地，实机迁移未验证；当前平台发布model-layout，圆盘扩大至150px（小屏136px），控制区布局重新整理；中央模型控制前端已开发，操作预览可用；模型推理/实机闭环尚未接入；姿态数值/四元数位于左侧；#34用户范围0–30°已开放；修复拖动成功提示造成的布局跳动；新增滑块拖动实时控制和编码器位置显示，隐藏电流负载温度；取消额外跟随/负载/漂移拦截，加速度设0，4.0–8.4V检查、50Hz发令及实际统计、3秒站姿任务和±5°到位判定；代码部署已核验，实物仍有跟随失败，不能称3秒站稳；ToF/摄像头故障未复测 | [网络与观测平台](04-网络与观测平台.md) |
| 摄像头 | 09-30 IMX219内核探测错误-5，mediad循环退出、8443不可用；此前1080p约30fps仅为历史成功，当前需排查连接/供电等 | [摄像头实测](05-摄像头实测.md) |
| ToF | 09-30 tofd运行但未探测到传感器，平台超时无帧；原因未确定 | [ToF实测](06-ToF实测.md) |
| HAT | 10-02外接MS901M修复后J7/UART9双向通信通过，约199Hz、零校验错误，±4g/±2000°/s量程各4次应答；平台已切ttyS9并验证约197Hz streaming； 最新模型动作缺8颗/乱码，电压6.7–7.2V，故障后15在线/全失能，IMU观测17.34°与用户竖直不一致；此前全离线0字节是历史阶段； 10-02板载BMI088在I²C3的0x19/0x68读取200组、零I/O错误，恢复原寄存器，未接平台；10-02独立整理；用户反馈输出4.95V、可启动主板，R33已短接；平台ttyS2已接入，最新30秒22491/22500有效反馈（缺失0.04%），1499/1500轮齐全，无新增UART错误，平台15在线；静止只读，恢复根因及运动稳定性未验收 | [HAT实测](07-HAT实测.md) |

## 证据与历史

- 2026-10-10 HAT主动散热选型：无专用FAN口，建议低电流5V两线风扇，J4取电需核对2A总预算，也可独立降压供电；仅方案、未安装验证，见[07-HAT实测](07-HAT实测.md)。

- 2026-10-02 外接MS901M接HAT J7/UART9方案已复核，尚未接线配置或验证，见[HAT实测](07-HAT实测.md)。

- 2026-10-02 HAT记录统一整理到[07-HAT实测](07-HAT实测.md)，包含接线供电、R33/R32、故障时间线、串口配置及原始附件；原01/03/04保留历史，不再重复维护HAT专属流水。

- 2026-10-02 核对R33为TTL DATA串联150Ω，另有TH1标称100Ω；用户短接后只读对照显著改善，500轮仍有1轮超时缺回包，尚非完全修复，见[舵机实测](03-舵机实测.md)。

- 2026-10-02 已补HAT舵机三线接法（GND/V+/DATA→J13或J14的1/2/3），后续HAT ttyS2已实测全部ID曾应答但明显丢包，通信稳定性未通过，见[舵机实测](03-舵机实测.md)。

- 2026-10-02 用户反馈HAT已到货；已记录首次供电方案，照片丝印C1与接口方向已对照PCB，复核J2/J9为麦克风端子、J13/J14可输入电源，背面J4测点已定位，用户反馈输出4.95V且连接主板后可启动；带载电压、长期及舵机动作稳定性未验收，见[主板与接线](01-主板与接线.md)。

- 2026-09-30 核对HAT降压路径：固定5V/2A级；LM5050-1供电在5V下限附近，启动/带载余量待实测，见[主板与接线](01-主板与接线.md)。

- 2026-09-30 官方HAT舵机口J13/J14与转接板用途已作资料核对，见[舵机实测](03-舵机实测.md)；未完成HAT舵机实物通信测试。

- 2026-09-30 核对官方HAT J5/J6针脚与默认0Ω配置，并确认J7可配置为Radxa UART9引出，见[主板与接线](01-主板与接线.md)末节；仅设计资料核对，未改焊或实测。

- 2026-09-29 补录：[ToF接线方案与核对边界](06-ToF实测.md#接线表2026-09-29-补录)。I²C4通信已实测，供电取脚、实际电压及LPn接法待确认。

- [摄像头实拍附件](附件/摄像头/README.md)：调整前、手动曝光对照、最终配置实拍，以及SHA-256校验值。
- [09-13 独立 IMU 测试原报告](历史归档/2026-09-13-主板与IMU独立测试原报告.md)：只代表当时，旧IP和线色不可当作当前状态。
- [09-25 拆分前综合记录](历史归档/2026-09-25-拆分前综合记录.md)：完整原文快照，用于追溯，后续不再追加。

## 与其他资料的区别

本目录是我们的实测结论入口。以下文件仍保留各自用途，通过这里查找，不把其中的设计、旧记录或软件测试混入当前硬件结论：

- [原有调试记录](../../../microduck-replica/调试记录.md)、[构建日志](../../../microduck-replica/构建日志.md)、[踩坑记录](../../../microduck-replica/踩坑记录.md)：含其他日期的操作手册、机械装配和项目历史，不能自动套用于当前这块主板。
- [Web 部署维护](../../deploy/README.md)：部署命令和服务说明；本机实测结果在本目录网络与观测平台记录。
- [手机软件测试](../../../mobile/TEST-RESULTS.md)：包含模拟器、构建与软件验证，不是整机实测。
- [App/BLE 设计资料](../../../mobile/design/BLE-WIFI-0.5.0.md)、[舵机姿态接入](../../../mobile/design/SERVO-POSE-0.6.0.md)、[共享显示标定](../../../mobile/design/SHARED-CALIBRATION-0.7.0.md)：保留实现细节，其已确认实测摘要已归入主题记录。
- 厂商规格、选型、采购和生成的接线示意图仍在原目录；示意图不能替代实际导通验证，文件名标“作废”的图禁止使用。

## 后续记录方式

1. 新实測追加到对应主题，注明日期、设备、条件、操作、结果与未验证项，并更新本页状态。
2. 区分“工具直接实测”“用户反馈”“推测”；成功、失败都保留，不用后来的结果改写历史。
3. 记录配置变更、备份和回退位置；不记录密码、绑定码等凭据。
4. 抓帧和日志等证据放在本目录附件中，注明来源。照片清晰、设备识别成功或数据出包，都不等于完整硬件功能验证。
5. 历史归档只追溯；旧入口文件只跳转，避免多处维护不同版本。

2026-10-09纠正：误将OTA清单发布为0.9.9/code28；已恢复现有0.9.10/code29原APK（6774697字节，SHA256 08caf138846713ed6e451761514749503e9d7e9fe3a415122d94f0e3b97eaeb4），公网签名/清单一致性/APK长度哈希/下载页均核对通过。证据mobile/releases/visual-0.9.9/restore-0.9.10.json。未重新构建0.9.10、未验证真实手机安装或主板动作。

2026-10-10 脚底打滑：用户反馈一走就摔；离线摩擦15工况对照复现低摩擦下起步失稳，未证明同向低头前摔或实机唯一根因，已打开mu0.2预览。见03末节与附件/平台/2026-10-10/foot-friction-sim/。未操作实机。

2026-10-10 脚滑优化最终：短测改善但四个偏置候选均丢长时正常行走成功例，全部拒绝、selected=null，未部署；详见03末节及RL artifacts/walk-slip-repair-20261010/final-gate.json。
