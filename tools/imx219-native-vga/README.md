> 恢复复核：同一设备当前IP **192.168.137.101**；builtin imx219、原启动配置、Observer active、15舵机卸力确认；实际MPP JPEG下载解码为640×360（顺时针旋转后360×640）。图像仅.local私有文件，不入库。

# IMX219 传感器 VGA 候选适配（2026-10-20）

状态：板端编译、模块加载、芯片ID读取成功，但两次管线绑定测试失败，尚未采集到 VGA 帧；不能发布为已支持。测试启动配置已撤销，恢复原驱动。

## 方案

向旧 Rockchip 驱动增加第三种模式：中心 (1000,752) 的 1280×960 ROI，0174/0175=3 的 2×2 binning，传感器输出 RAW10 640×480。目标帧率沿用 vendor 30 fps 时序（VTS=0x06e6），实际帧率待测。该方案缩小视野，不保留现有 1080p 模式的完整视野，也不使用 ISP 拉伸生成 VGA。

原驱动 s_stream 会重写 ROI 和输出尺寸，因此 VGA 模式跳过该段，保留模式表的独立输入/输出尺寸；set_fmt 更新 ROI；测试图尺寸使用输出尺寸。原1080p、全分辨率寄存器表逐字未变。独立驱动名 imx219_microduck，传感器报告名称仍 imx219，以保留 Rockchip IQ 匹配。

## 来源和可复现构建

- 基础源码：[Armbian pinned source](https://github.com/armbian/linux-rockchip/blob/372ce582b94a3360a29727c3c49c88bb7509da22/drivers/media/i2c/imx219.c)，SHA256 `c98f6e38b1efa41cd8eccb05815a838a2d7ceca945ff23084115ca306a93be63`。其两种模式与板端一致，但尚未证明就是板端内核构建使用的精确提交。
- VGA 序列来自 [Linux v6.1 IMX219](https://github.com/torvalds/linux/blob/v6.1/drivers/media/i2c/imx219.c)，保留 GPL-2.0 来源，添加 vendor 帧时序和 RAW10 配置。
- 在目标板运行 `sh tools/imx219-native-vga/build.sh /home/radxa/imx219-new-build`，输出目录必须不存在。脚本只下载、校验、应用补丁和编译，无安装/加载/启动配置操作。

## 已验证

板：microduck-e6fc56 / 192.168.137.239，内核 `6.1.115-vendor-rk35xx`；匹配 headers 和 Module.symvers，CONFIG_VIDEO_IMX219=y。

候选位于 `/home/radxa/imx219-adapt-stage/imx219_microduck.ko`，CC / MODPOST / LD 成功。缺少 vmlinux 跳过 BTF，不影响模块生成。普通 PATH 没有 modinfo，改用 /sbin/modinfo 验证成功。

- vermagic：`6.1.115-vendor-rk35xx SMP mod_unload modversions aarch64`
- 模块 SHA256：`253735d241114c51a63b497b7c83b97adfa766ce4578562d544c115cd6014474`
- 候选 C 源 SHA256：`87b7812a5bea18bf43870a46929cccb000e4bc93fa1e609002cba7bf63fcc4cd`
- 静态检查：ROI 1280×960、输出640×480、binning=3/3、RAW10、VTS正确；原两种寄存器表未变。
- 上述为第一阶段构建记录；后续加载/重启测试见下文。

## 待验证与发布门槛

编译通过不等于硬件可用。先确认15舵机卸力、退出自主，再停止图像使用者，验证模块加载/符号版本、旧驱动解绑及候选绑定、media graph 重新注册是否正常。必须检查传感器与 CSI/ISP sink 实际都是640×480，RAW帧长度、连续出帧、实际fps和内核CSI错误；不能只看最终JPEG尺寸。随后测ISP/3A和画面比例。

预先准备恢复 builtin imx219、重新建立原1080p图链、启动原v4b服务的回退。热解绑是否可靠尚未验证；不要直接安装开机加载，也不要使用force加载。完成原始采集验证后，才联动更新 Observer/MPP/RGA/视觉IPC的480行尺寸。此补丁不代表已完成Observer VGA部署。

## 板端继续验证（2026-10-20）

每次停服务/重启前确认15舵机在线且torque=0、反馈新鲜、自主off和控制idle；未发运动指令。

1. **热切换失败**：insmod成功，旧驱动解绑成功；候选probe读出Model ID 0x0219，但注册图链时报 `No link between dphy and sensor`，绑定返回EIO。旧驱动回绑未恢复sensor media entity，重启后恢复正常。
2. **启动选择候选也未建链**：用独立 `microduck,imx219-vga-test` compatible避免内置驱动提前占用；候选module在systemd阶段加载并成功绑定，但CSI在10秒左右已probe并清理未就绪subdev，候选23秒才加载，未建立传感器entity，设置格式返回EINVAL。未进入采集阶段，不能评价VGA帧率或图像质量。
3. 一次测试overlay只改变sensor compatible，启动测试服务先恢复原armbianEnv再insmod；随后已disable测试服务并恢复原启动配置。未将候选设为常驻驱动。测试工件保留在板侧 `/var/lib/microduck-imx219-test`，服务已禁用。

源码解释：[同一固定提交的 CSI DPHY 驱动](https://github.com/armbian/linux-rockchip/blob/372ce582b94a3360a29727c3c49c88bb7509da22/drivers/phy/rockchip/phy-rockchip-csi2-dphy.c) 在unbind回调只清sensor指针、不减少num_sensors；bound仅对num_sensors==1启用link。另有GKI分支会跳过当时尚未绑定驱动的sensor。它们与板上现象一致；未证明该提交就是板端构建的精确源码。

因此不能继续把“单独编译sensor模块”当完整适配方案。下一阶段应取得匹配内核构建来源，在原内置IMX219驱动中加入VGA（保留原sony compatible），或连同CSI注册/移除逻辑修复并验证；需完整可回退内核构建与启动验证。当前补丁是实验模块版本，compatible已改为测试名称，不可直接套用为内置驱动补丁。

启动测试模块SHA256：`cc2fb847bd936d9a037967c0e84a6a955f76a303ee54ea9cb52fe0ade6c7dc33`。原armbianEnv SHA256：`a2431830048a19780588f34de57b7c86118edc209b8ab09e6f2833719043edd2`。

证据位于 `docs/实测记录/附件/摄像头/2026-10-20/native-vga/`。日志中没有图像；两次都未成功采集，不能声称640×480传感器输出已实测。
