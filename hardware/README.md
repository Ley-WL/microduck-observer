# MicroDuck 硬件

后续硬件设计资料统一存放在这里，按模块和版本归档。

| 模块 | 当前版本 | 内容 | 当前状态 |
| --- | --- | --- | --- |
| [舵机电源Hub](power-hub/README.md) | [Rev N / ECO1](power-hub/revN-eco1/README.md) | 三路AMP2.0电源/共DATA；无开关，保留C1去耦；两侧Ø3mm电池焊盘 | 源ERC/DRC/连通性通过；立创客户端更新预览/DRC、实物装配与负载验证待完成 |
| [imu_to_dxl](imu_to_dxl/README.md) | 上游 `14b6e768a3e9` | 身体姿态传感器／IMU转舵机协议板；KiCad源工程与生产资料 | 原样导入、哈希及ZIP完整性通过；本地设计检查和实物验证未进行 |
| [banana_pcb](banana_pcb/README.md) | 上游 `14b6e768a3e9` | 电池转接板；KiCad源工程与生产资料 | 原样导入、哈希及ZIP完整性通过；本地设计检查和实物验证未进行 |
| [dxl_hub](dxl_hub/README.md) | 上游 `14b6e768a3e9` | 电力与数据分发板；KiCad源工程与生产资料 | 原样导入、哈希及ZIP完整性通过；本地设计检查和实物验证未进行 |

每个模块分别保存源工程、EDA工程、制造文件、BOM、效果图和验证记录。新增硬件模块也放到本目录，并更新此表。

实测、接线、部署和故障的主记录继续维护在 [实测记录](../docs/实测记录/README.md)，不把设计检查当成实物通过。
