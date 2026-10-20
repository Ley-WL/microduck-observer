# 舵机电源 Hub

当前方案：**[Rev N / ECO1：无开关＋C1去耦电容](revN-eco1/README.md)**。采购为三个CJT A2007WV-3P（C225336）座和一颗10µF/25V/X7R、1206电容；电容具体品牌可按规格替代。

- [最新立创EDA专业版工程](revN-eco1/eda/MicroDuck-Hub-RevN-Cap-ECO1.epro)
- [采购BOM](revN-eco1/bom/BOM.csv)
- [源板Gerber打样包](revN-eco1/manufacturing/MicroDuck-Hub-RevN-Gerber.zip)
- [完整归档资料包](packages/MicroDuck-Hub-RevN-ECO1-Complete.zip)
- [效果图](revN-eco1/previews/pcb-3d.png)

此处“当前版本”指用户确认的设计方案；**立创客户端新预览/DRC和实物验证仍待完成**。导入前阅读版本说明，避免更新时删除安装孔或覆盖GND线宽规则。

今后的电源Hub修改从本目录继续，新增版本时保留旧版并更新当前入口。2026-10-20以前的A–N生成过程、开关版及诊断中间稿留在 `microduck-rl/hardware/power-hub/` 历史来源目录；不再向那里新增正式版本。HAT相关实测见 [07-HAT实测](../../docs/实测记录/07-HAT实测.md)。
