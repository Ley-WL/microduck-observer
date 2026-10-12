# 2026-10-12 Web掉线/延迟核查

用户使用现场http://10.10.161.13:8877/，开发机不在该LAN；通过5722 SSH只读测主板内部服务和Wi-Fi。服务PID1950/NRestarts0，08:45起运行；20次HTTP loopback中位11.14ms/最大96.72ms，旧订阅20秒连续2400余包/10心跳，最大收包间隔39.87ms，未掉线。此非现场浏览器端到端延迟，也不能证明长期不断。

主板网关10次ping无丢包、2.13～108.94ms/均38.38；Wi-Fi省电已经off，初次2452MHz/−62dBm，接收速率8.6Mbps。wpa_supplicant日志反复reason8 DISCONNECTED并在同SSID多个BSSID与2.4/5GHz关联，同时有WNM Disassociation Imminent/Preferred List通知。NetworkManager wlan0 unmanaged，实际netplan独立supplicant管理。说明无线关联确实在掉/换AP，不能等同服务重启；没有空中帧或网管策略证据，不称唯一驱动或强制漫游根因。未锁BSSID、改Wi-Fi/驱动/省电或重启服务。

页面旧订阅orientation50/raw50/joints20/tof20，原payload 198.47KB/s（其中关节150KB/s）；新orientation50保留、raw10/joints10/tof10，主板同20秒订阅对照103.80KB/s，下降47.70%。不降硬件采样/模型推理/保护或日志内容；网络减少不冒称用户延迟已降低。UI暂停原6秒timer先于缓冲消息可能误关闭，现隐藏不超时关闭、恢复/长event-loop暂停给6秒收包窗口，但sample年龄未重置；前台连续6秒无消息仍关闭。每重连attempt独立generation，旧socket回调不得覆盖新连接，5秒握手期限/单次retry/close code原因保留。

17文件66前端检查与生产构建通过，新增真实store模拟旧回调/握手取消及watchdog隐藏恢复/连续失联测试。frontend-20261012-094845纯静态部署通过HTTP SHA/原PID/模型增益标定保持/tofd rsyslog masked，空闲15卸力；未后端更新/代发动作/训练。新20秒loopback仍连续、最大收包间隔40.51ms。用户刷新加载；真实LAN改善尚待验收，现场无线掉线没有因网页修复被宣称解决。可先用单AP/稳定独立热点对照，或请网管核查设备频段引导/踢离事件。

board-readonly*.json为两次原始报告，wifi-readonly/wifi-events含系统与关联日志，collect*.py为不含凭据的只读脚本，verification.json为摘要。部署证据见旁目录frontend-20261012-094845/deployment.json。第一次读取ws_reader模块路径不在current导致采集未执行，后改内嵌stdlib reader完成，不混作服务异常。
