# IMU实测

[返回实测索引](README.md)

> 最新结论（2026-09-25）：已经恢复出包，但加速度在翻面前后保持同值、模长异常，融合姿态不可靠，不能认定恢复正常。下文早期成功结果仅代表当时测试。

接线表见 [主板与接线](01-主板与接线.md)，最新线色与接地复测见本文 2026-09-24 记录。

## 当前进度与软件状态

- 用户已确认正确 6、14 号导通；已给出上面的纠正接线表。
- 用户已确认按纠正表重接并断电后上电；2026-09-13 已成功读取真实 IMU 数据，地址 0x4B。
- 诊断脚本：项目 scripts/bno085_diagnostic.py，板上 /home/radxa/microduck-debug/bno085_diagnostic.py。使用 Linux i2c-dev 直接读取 SHTP 数据，不控制舵机；结束时关闭本次启用的传感器报告。
- 已请求加速度、角速度、旋转向量各 20000 微秒报告间隔。实测 12.001 秒：加速度 768 份（63.99Hz），角速度 597 份（49.74Hz），四元数 596 份（49.66Hz）；I/O 错误 0，未解析报告 0。加速度实际频率高于请求值，不应笼统宣称所有报告都严格 50Hz。
- 产品 ID 响应原始数据：f8010206dca49800c100000006000000。
- 最后一帧加速度 [-0.44922, -7.09375, 7.08594] m/s²；角速度 [-0.00391, 0, -0.00391] rad/s；四元数 xyzw [-0.33398, 0.18835, -0.40179, 0.83154]。数据量级合理，但尚未完成安装方向校准、运动响应测试或长期稳定性测试。
- MicroDuck 原程序尚未接入该 BNO085；当前成功仅表示独立 IMU 通信和数据读取验证通过，不表示整机控制或模型已验证。
- 系统 Armbian 26.2.1 Trixie，vendor 内核 6.1.115，hostname microduck，用户 radxa。
- 最近 IP 192.168.31.190；SSH 主机密钥已与先前 192.168.31.191 验证一致，现有助手使用 HostKeyAlias=192.168.31.191。不要绕过密钥检查。
- 已启用 /dev/i2c-4，/boot/armbianEnv.txt 中 overlay_prefix=rk3568、overlays=i2c4-m0。
- 排查时添加了 user_overlays=microduck-i2c4-pullup-test，文件位于 /boot/overlay-user/，用于启用内部上拉；它目前仍保留，不是已撤销的修改。原配置备份 /boot/armbianEnv.txt.before-pullup-test。接线纠正后应重新评估是否需要保留。
- 检测脚本 /home/radxa/microduck-debug/probe-imu.py，对 0x4B/0x4A 分别读取 4 字节。此前完全空载返回 Errno 6，错误接线曾返回 Errno 110；这些结果不证明 IMU 损坏。
- Windows 管理辅助脚本 D:/Users/wl/Downloads/MicroDuck-Flash/run-board-admin-190.py；登录凭据保存在该目录已有 board-login.txt，不复制密码到项目文件。

## 2026-09-21 重接后的独立复测

- 用户重接后请求测试。原 IP .190/.191 不可达；发现主板现为 192.168.31.193，使用 HostKeyAlias=192.168.31.191 和 StrictHostKeyChecking=yes 验证已有 SSH 主机密钥通过，hostname 为 microduck。
- 运行板上 bno085_diagnostic.py --seconds 12 --refresh 2，正常退出；未控制舵机。
- 地址 0x4B，PRODUCT_ID f8010206dca49800c100000006000000。
- 实测 12.002 秒：加速度 769 份（64.07Hz），角速度 597 份（49.74Hz），四元数 597 份（49.74Hz）。I/O 错误 0，未解析报告 0。
- 最后加速度 [-0.30078, -2.85547, 9.65625] m/s²，角速度 [0, 0, 0] rad/s；四元数 xyzw [-0.02551, -0.14227, 0.98682, 0.07324]。传感器坐标系欧拉角 roll=-16.54°、pitch=1.69°、yaw=171.26°，未经安装方向校准。
- 结论：此次接线支持正常独立 IMU 通信和数据读取。尚未完成运动响应、长期稳定性或 MicroDuck 原程序集成验证。

## 2026-09-24 IMU 无采样排查（未恢复）

- 主板 192.168.31.193 在线，已有 SSH 主机密钥验证通过；i2c4-m0 与内部上拉 overlay 仍启用。
- microduck-observer 运行，但 IMU unavailable、counts 为空；可读取 0x4B 产品响应 f8020206dca49800c100000006000000，I/O 错误为0。
- 停止观测服务排除争用，运行原独立诊断12.001秒：无任何加速度/角速度/四元数报告，I/O错误0。
- 仅针对IMU发送一次SHTP可执行通道软件复位（通道1，载荷01），等待2秒，再测12.002秒，仍无报告。不能据此断言芯片损坏或接线全部正确。
- 两次诊断后均恢复microduck-observer；最终HTTP health可达，IMU仍 unavailable。未改接线、overlay、采集代码或标定文件，未发送舵机控制指令。
- 下一步需要用户正常关机、彻底断开供电后重新上电，排除传感器上电/模式采样异常；如仍失败，需按实物记录检查供电、PS0/PS1及实际接触情况。
- 同时观测服务报告舵机串口 Serial unavailable；本次尚未验证舵机接入状态。

### 2026-09-24 用户重新上电后的复核

- SSH uptime确认主板重新启动，观测服务在线但仍无IMU采样。
- 当前部署链接为releases/20260924-imu-soft-reset，其BNO085.start已包含软件复位；本轮未改动部署代码。
- 暂停观测服务独占I2C，发送Set Feature并查询Get Feature：加速度返回fc01000000000000000000000000000000（报告间隔0）；角速度返回fc02000000204e00000000000000000000，旋转向量返回fc05000000204e00000000000000000000（两者间隔20000微秒）。只收到通道2控制响应，没有通道3/4采样。
- 说明控制通信可用，角速度/旋转向量报告配置可回读，但传感器采样仍不工作。尚不能确定供电、模块内部传感器或固件状态等具体原因。诊断后已恢复观测服务。
- 下一步需核对当前实物接线照片，并测量IMU模块VCC与GND之间实际供电，重点排除接触及模式引脚问题。

### 2026-09-24 最新实物照片与供电测量

- 用户报告IMU VCC-GND测量为3.28V，静态电压在模块供电范围内，不代表已验证上电波形或内部电源。
- 最新照片IMU端实际线色：VCC红、GND橙、SCL黄、SDA绿、PS1蓝、PS0紫。此前线色不能继续套用。
- 最新主板照片相对记录标准方向旋转180度：天线在上、USB在左、排针在右，因此这张照片上从上往下数，内排为左列（靠芯片）、外排为右列（靠板边）。1号在上端内排。
- 主板插头遮挡部分孔位，照片不能可靠确认每根线实际插孔。IMU焊点裸露线芯偏长，有散股/锡堆，照片不足以确认短路或虚焊。下一步断电后实测PS1-GND、PS0-GND导通，必要时检查相邻焊点及端到端导通。

### 2026-09-24 模式接地修正后上电复测

- 用户先反馈PS1-GND、PS0-GND不响；拔开主板端后，蓝/紫/橙三根线的端到端测量均响。按最新照片方向重新核对并插牢后，用户确认两组接地测量响。
- 用户再次上电，SSH uptime确认主板运行约1分钟、microduck-observer active。约10秒内三次HTTP健康检查均为IMU unavailable：counts为空，sampleAgeMs=null，ioErrors=0，无imu.raw或imu.orientation。
- 接地导通修正后仍未恢复数据，不能把接地问题认定为全部故障原因，也不能仅由此认定芯片损坏。供电此前实测3.28V，控制通道产品查询可响应。

### 2026-09-24 内部错误查询与唤醒测试

- 用户断电检测：SCL-SDA、RST-GND、BOOT-GND在200Ω档均显示OL，表笔短接0.1Ω。仅说明未测到低阻短路，不代表完整功能验证。
- 主板再次上电后，停止观测服务独占读取。参照CEVA官方sh2.c/sh2.h（https://github.com/ceva-dsp/sh2），发送Get Errors命令1，查询severity 0/1/2/3；响应source均为255，表示本次查询没有错误记录，不是“错误255”。
- 设置加速度、角速度、旋转向量报告后，仍只收到控制通道2响应，没有采样通道3/4。
- 参照官方sh2_devOn，发送可执行通道1 ON命令2，再运行原独立诊断8.001秒，counts为空，io_errors=0，未恢复。
- 诊断后恢复microduck-observer并确认HTTP health可达，IMU仍unavailable。未修改固件、持久标定、驱动代码或舵机控制。
- 建议下一步使用已知正常的BNO085模块在相同主板/软件下交叉测试，定位模块问题；现有证据尚不足以确认芯片损坏。

### 商家1002说明书核对

- 已阅读用户提供的D:/Users/wl/Downloads/1002+产品说明书+V0.0.0.doc，包括嵌入的接线图、正反面图和代码截图。
- 商家明确：供电2.4–3.6V，建议3.3V；IIC模式PS1=0、PS0=0；默认地址0x4B，AD0拉低改0x4A且需复位；PS焊盘与PS引脚相通。
- 示例使用ESP32-S3，SDA=6、SCL=7、INT=5、RST=4；这些是ESP32的GPIO号，不是Radxa物理针脚号。代码截图明确INT与RST均标注Optional for I2C。
- 因此当前六线方案在引脚功能上与说明书一致：VCC→物理1，GND→6，SCL→28，SDA→27，PS1→9，PS0→14。不接INT/RST不构成与商家示例的必需接线冲突。
- 此核对不代表确认实际焊点/接触全部正常，也不能由无采样直接认定模块损坏。

## 2026-09-25 恢复出包但加速度异常

- 用户报告Pitch转到90度后静止会变小；水平放置截图Roll50.39/Pitch0.93/Yaw16.26，显示安装方向修正-90度，有保存的相对参考。
- HTTP健康检查streaming、采样约50Hz、无I/O错误，不等于物理测量有效。初次静止约18秒加速度一直[1.01171875,5.10546875,1.79296875]m/s²，模长5.504918（静止应接近9.8）。姿态精度状态0。
- 请求用户翻面，用户确认放稳；随后9秒内4次检查加速度仍为完全相同的三个值，四元数相较翻面前改变。序号和时间戳持续更新。
- 停止观测服务，原独立诊断8.001秒确认512份加速度、397份角速度和四元数、I/O错误0；加速度仍[1.01172,5.10547,1.79297]，排除仅网页显示/观测服务缓存旧加速度。诊断后恢复服务。
- 结论：不能认为IMU完全恢复；加速度测量/报告链路异常，可能导致融合姿态不可靠。尚未定位内部硬件、固件状态或底层协议原因，不应仅用归零或90度欧拉角奇异解释。

## 2026-09-29 蓝色串口 IMU：用户交换 RX/TX 后检查

- 用户反馈已交换RX/TX，请求读取。此次针对用户图片中的蓝色TTL串口IMU，具体型号未知；不是上述紫色GY-BNO085，不沿用旧I²C协议与故障结论。实际四线连接、电平及模块工作模式未实物核对。
- 工具实测主控192.168.31.193在线；uart2-m0已启用，Pin8/10对应ttyS2，内核pinmux为uart2m0-xfer。robotd仍为fake模式。ttyS1被蓝牙hciattach占用，未操作它。
- 发现ttyS2的serial-getty为active，内核命令行还有console=ttyS2,1500000。临时停止serial-getty，保存原termios后，以8N1、无流控依次测试9600/115200/57600/38400/19200/4800/230400，各接收2秒，全部0字节；未向IMU发送配置或查询命令。通用0x55/11字节协议校验只作为候选检测，并未认定模块采用该协议。
- 结束在finally恢复原termios并启动原serial-getty，复核为active；未修改启动配置，内核串口控制台未解除。读取/proc/tty/driver/serial显示ttyS2累计rx:0，未见硬件接收字节。
- 结论：交换后仍没有收到数据，不能仅归因于波特率或认定模块损坏。需核对模块VCC/GND、模块TX到主控物理Pin10的实际连接、3.3V信号电平，以及是否为主动输出模式；推荐后续只接VCC/GND/TX→主控RX三线进行只读验证，暂不接模块RX以隔离主控控制台输出。实际供电及型号资料尚缺。
- 证据：[各波特率接收结果](附件/IMU/2026-09-29/交换RX-TX后串口检查.json)。本轮无新有效IMU测量。

### 2026-09-29 接入后无法启动反馈与接线照片核对

- 用户反馈“接上去就开不了机”，尚未确认是指示灯不亮还是网络不可达，亦未确认拔掉IMU后的启动对照。不能据此认定供电短路或启动串口受干扰。
- 用户提供[蓝色串口IMU接线照片](附件/IMU/2026-09-29/蓝色串口IMU接线照片.jpg)。照片芯片面朝上、USB在上、天线在右、40Pin在下：从右侧天线端往左编号，靠芯片内排1/3/5…，靠下方板边外排2/4/6…。
- 照片目视：IMU上排丝印从左到右D0/VCC/RX/TX/GND/D1，四个插头看起来覆盖VCC/RX/TX/GND；TX为紫线、GND为蓝线，两根浅色线接VCC/RX，但跨线遮挡使两端对应不可可靠确认。未发现足以确认整体错一位的证据，也未实测焊点是否短路。
- 主控插头遮挡针位且透视较大，不能确认紫线/蓝线具体孔号，不能宣布接线正确。按此照片方向，计划VCC接内排右起第1个Pin1、GND接外排右起第3个Pin6、IMU TX接外排右起第5个Pin10，IMU RX先不接。下一步保持断电，提供主控40Pin垂直俯拍核对电源孔位；未核对前不建议盲目换线或反复上电。

### 2026-09-29 主控40Pin近照复核

- 新照片：[主控40Pin近照](附件/IMU/2026-09-29/主控40Pin近照.jpg)。正面朝上、排针左、USB右、天线下，为标准编号方向。
- 按可见排针间距及底端定位，最下方内排浅灰线看起来位于Pin1（3.3V）；蓝线位于外排由下往上第3排Pin6（GND）；紫线位于外排第5排Pin10（UART2 RX）。与上一张模块端蓝线GND、紫线TX相符，照片未显示这三根线有明显编号/方向错误。另一根浅色线的黑色端子看似悬空，不能仅凭图确认其端到端对应。
- 此为照片判断，不是万用表导通或电平验证；焊点短路、接触和实际供电未排除。建议不再盲目交换RX/TX；若接入影响启动，断电后先拔开IMU TX到Pin10的紫线，仅保留确认的供电与地线做启动对照，以区分供电负载与串口相关因素。该对照尚未执行，启动串口干扰仍是待验证假设。

### 2026-09-29 仅接Pin1/6启动对照

- 用户明确当前仅连接Pin1（3.3V）和Pin6（GND），请求检查。工具通过SSH成功登录192.168.31.193，hostname=microduck；uptime复查80.51秒，boot_id=33e26c93-3095-499f-9257-93b43aa78000，说明此接线条件下主控已进入Linux并可联网。microduck-observer及tofd均active。
- systemctl仍为starting、failed单元0；启动作业等待dev-ttyFIQ0.device及serial-getty@ttyFIQ0.service，不把SSH可达表述成所有启动作业完成。此次未更改系统配置。
- 仅供电接线下可进入系统，尚不能证明IMU自身正常或供电电气指标合格；此前接信号线后的启动问题仍待对照，串口干扰并未被证实。没有信号线时不进行IMU数据读取。

### 2026-09-29 释放Linux UART2并重启验证

- 在用户反馈仅接Pin1/6、主控SSH可达的条件下，按用户请求处理串口占用。备份于主控`/home/radxa/microduck-debug/uart2-console-backup-20260929-215254/`，含原armbianEnv.txt及两项服务原状态JSON（原均enabled）。
- `/boot/armbianEnv.txt`仅将`console=both`改为`console=display`，保留uart2-m0及其他overlay；mask --now serial-getty@ttyS2.service。另mask --now原先等待不存在ttyFIQ0设备的serial-getty@ttyFIQ0.service，避免启动作业等待。
- 执行正常systemctl reboot后SSH重新可达，boot_id=9de78e31-4121-4347-9b58-39def15b0aba；/proc/cmdline仅console=tty1，无ttyS2控制台；两项getty均masked，ttyS2为root:dialout；系统running，观测及tofd均active。
- 此次仅验证Linux串口占用解除及仅供电条件下重启成功；没有修改U-Boot环境或固件，不保证启动加载器阶段不受IMU输入影响。尚未重接TX复测，不代表收到IMU数据。
- 后续：断电后只增加IMU TX（紫线）到主控Pin10，模块RX仍不接，启动后尝试接收；若接TX又不能启动，停止反复上电，转查U-Boot输入或改用独立串口/USB-TTL方案。
- 回退：从上述备份恢复armbianEnv.txt，unmask两项serial-getty，按service-state.json恢复原启用状态，再正常重启。

### 2026-09-29 官方资料核对：建议迁移UART4_M1

- 用户询问Pin8/10替代方案。重新读取Radxa官方radxa-docs/docs仓库hardware-interface.md：明确警告UART2_M0同时为U-Boot控制台，输入会中断启动，不建议作普通串口。此前仅关闭Linux控制台无法解决此启动阶段问题；当前用户现象与该机制一致，但尚未独立证明唯一原因。
- 官方40Pin表：Pin16=UART4_RX_M1(GPIO3_B1)，Pin18=UART4_TX_M1(GPIO3_B2)。建议IMU TX→Pin16，IMU RX→Pin18（只读测试可先不接RX），VCC/GND沿用Pin1/6。标准正面方向外排由下向上第8/9个。与ToF Pin27/28无针脚冲突。
- 此为拟采用方案，尚未检查主控uart4-m1 overlay文件、启用或实测；不可只换线就宣称可读。此次SSH到192.168.31.193超时，需要用户断电拔开紫色TX线后恢复主板上线，再配置UART4。
- 官方来源：https://docs.radxa.com/zero/zero3/hardware-design/hardware-interface 。未采用官方针对Radxa OS的rsetup/u-boot-update步骤修改当前Armbian引导器。

### 2026-09-29 用户接线后联网复核

- 用户回复“接上了”并请求检查在线状态，未逐脚重述实际接法。工具SSH成功连接192.168.31.193，hostname=microduck、刚启动不足1分钟，systemctl为running。不能仅由“接上了”将Pin16/18实物接线认定为已验收。
- 当前仅见ttyS1/ttyS2，armbianEnv.txt尚无uart4-m1；UART4尚未启用，本轮只确认主控在线，未读取新串口IMU数据、未更改配置。

## 2026-09-29 UART4启用成功，蓝色串口IMU连续数据验证通过

- 系统自带rk3568-uart4-m1.dtbo，dtc反编译metadata明确ZERO3 RX物理Pin16、TX物理Pin18，目标uart4与uart4m1_xfer；启用前Pin16为MUX/GPIO未占用。
- 将uart4-m1追加到/boot/armbianEnv.txt的overlays，保留其他配置；原文件备份`/home/radxa/microduck-debug/armbianEnv.before-uart4-20260929-221314.txt`。正常重启后SSH可达，系统running、观测和tofd均active，出现/dev/ttyS4；debugfs确认GPIO3_B1/B2复用到fe680000.serial的uart4m1-xfer。回退：恢复该备份并正常重启（会恢复到已禁用Linux串口控制台的状态，不回退更早变更）。
- 只接收不发送IMU命令，以8N1无流控测试9600和115200。9600收到乱码；115200两秒21472字节，随后10秒107491字节。测试关闭端口并恢复原termios，没有配置持久IMU采集服务。
- 初始通用0x55/11字节探测误将本模块部分55 55帧视作type55，不能把输出的valid_wit_11byte_frames当作标准WIT协议识别。随后对全部原始数据按实测结构`55 55 type length payload checksum`重新校验，checksum为末字节前整帧字节和低8位。
- 十秒数据得到7961个完整校验通过帧，校验失败候选0；type01/length6为1990帧，type02/length8为1991帧，type03/length12及type06/length8各1990帧，约每类199Hz。开头10字节/末尾8字节为采集边界未完整成帧的字节，不能用本结果宣称有序号丢帧率验证。
- 最新结论：Pin16/18对应UART4已启用，115200下收到连续且校验一致的IMU串口数据；当前接线条件下主板可以重启。模块具体型号、载荷物理单位/轴向、静态精度及运动响应尚未核对；未将原始数值解释为已验证角度/加速度，也未接入观测平台。不能沿用此前紫色BNO085解析器。
- 原接线方案：VCC→Pin1、GND→Pin6、模块TX→Pin16，模块RX→Pin18（只读可不接）。用户未给出换线后的照片，当前只确认UART4通路实测成功，不认定RX线也已逐脚验收。
- 证据：[波特率探测](附件/IMU/2026-09-29/UART4波特率探测.json)、[10秒原始二进制](附件/IMU/2026-09-29/UART4-115200-10秒.bin)、[变长帧校验统计](附件/IMU/2026-09-29/UART4-115200-10秒.json)。

## 2026-09-29 MS901M接入独立观测平台并部署

- 用户确认品牌为正点原子，未提供型号；结合板上MS901M丝印、帧格式与公开镜像中的正点原子`atk_ms901m.c` V1.0（2022-06-21）核对，按MS901M协议实现。协议依据为 https://github.com/jyyy3901/esp-drone-ms901m 的 `hardware/ATK-IMU901模块资料（新资料）/2，程序源码/ATK-MS901M模块测试实验/精英STM32F103开发板/Drivers/BSP/ATK_MS901M/atk_ms901m.c`。这是厂商示例的第三方镜像，不把镜像仓库本身称为官方发布源。
- 工具直接读取寄存器：只发送量程读取请求，返回`55af0301030b`、`55af0401010a`，确认gyro索引3=±2000°/s、accel索引1=±4g。无模块配置、Flash保存、校准或运动指令。两向串口通信已能得到查询应答，但换线后具体物理接法仍无新照片逐线验收。
- 实现：新增ms901m.py增量帧解析、校验失败重同步、串口独占、断流重连；UART4 115200/8N1。四元数由Q0/Q1/Q2/Q3转平台xyzw；加速度按读取量程换算m/s²，角速度换算rad/s；量程未读回时不发布猜测单位的imu.raw。accuracy=null，不伪造BNO085精度级别。实际安装方向未校准，不发布身体姿态pose。
- 新release `/home/radxa/microduck-observer/releases/20260929-ms901m`。新增systemd drop-in `/etc/systemd/system/microduck-observer.service.d/imu-ms901m.conf`，设置MICRODUCK_IMU_DRIVER=ms901m、MICRODUCK_IMU_PORT=/dev/ttyS4、SupplementaryGroups=dialout、DeviceAllow=/dev/ttyS4 rw。只重启观测平台，未重启tofd/robotd或控制舵机。
- 前端型号显示由固定BNO085改为info.imuModel，复用现有实时姿态/IMU曲线。旧BNO085姿态参考及安装轴样本不适用于新模块，已备份后将共享标定imu参考清空、mounting.yaw设0；initialized=true作为已清除标记，防止旧浏览器本地标定重新迁入；舵机标定内容保留。
- 备份：主控`/home/radxa/microduck-observer/ms901m-backup-20260929/`含`previous-release.txt`和`calibration.json`。旧release为20260929-tof，未删除。回退：停止microduck-observer，移除本次新增imu-ms901m.conf，将current恢复指向previous-release.txt记录路径，恢复备份calibration.json及原文件属主/权限，daemon-reload后启动服务。若期间有新标定，应先保存当前标定再决定是否恢复旧文件。
- 软件验证：46项后端测试、28项前端测试通过，生产构建通过；涵盖真实107491字节回放、7961帧校验、坏帧重同步、单位/四元数顺序、未知量程抑制和过期状态。最后health有效性补充后，4项MS901M测试再次通过。测试不替代真实轴向和精度校准。
- 工具直接部署验证：首页、当前JS/CSS均HTTP200；health显示MS901M streaming、量程confirmed、底层各报告约198Hz，I/O错误、未解析报告、校验错误、丢弃事件均0。WebSocket实采12.024秒，imu.orientation 598条（49.736Hz）、imu.raw 597条（49.652Hz）、ToF162条（13.474Hz）；IMU样本最大ageMs 24.845（后端采样年龄，不代表端到端显示延迟）。序号严格递增；推送有意降采样，未宣称硬件无序号丢帧。
- 本轮加速度模长9.908–10.009m/s²、中位9.959；没有完成已知姿态旋转、动态响应或精度测试，不能据此宣布完整校准。浏览器自动打开超时，未目视确认页面渲染与交互；HTTP/WS与生产资源已验证。舵机仍报告Serial unavailable，本次仅恢复IMU观测，不代表整机可运动。
- 证据：[平台MS901M WebSocket及health验证](附件/IMU/2026-09-29/平台MS901M验证.json)。
