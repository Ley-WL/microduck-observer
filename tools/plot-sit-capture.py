"""Plot fresh servo register-67 targets versus measured positions."""
import json
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.font_manager import FontProperties
root=Path(__file__).resolve().parents[1]
p=root/'docs/实测记录/附件/平台/2026-10-10/bidirectional-sit-inspect'
d=json.loads((p/'timeline.json').read_text(encoding='utf-8'))
c=json.loads((p/'calibration-capture.json').read_text(encoding='utf-8'))
start=d['samples'][0]['at']+7.37
rows=[r for r in d['samples'] if -.3 <= r['at']-start <= 6.5]
font=FontProperties(fname='C:/Windows/Fonts/msyh.ttc')
def angle(row,id,key):
    servo=next(s for s in row['servos'] if s['id']==id)
    return (servo[key]-c['joints']['references'][str(id)])*360/4096*c['joints'].get('directions',{}).get(str(id),-1)
t=[r['at']-start for r in rows]
fig,axes=plt.subplots(2,1,figsize=(10,6),sharex=True,layout='constrained')
for ax,id,title in zip(axes,[23,24],['左膝：初期目标反复变化，随后落座稳定','左踝：同步反复调整']):
    ax.plot(t,[angle(r,id,'target') for r in rows],color='#e07a21',label='舵机实时目标（寄存器67）',lw=1.6)
    ax.plot(t,[angle(r,id,'position') for r in rows],color='#2276bd',label='实际编码角度',lw=1.6)
    ax.axvspan(0,2.1,color='#e07a21',alpha=.09)
    ax.set_title(title,fontproperties=font)
    ax.set_ylabel('角度（°）',fontproperties=font)
    ax.grid(alpha=.2);ax.legend(prop=font,loc='upper left')
axes[-1].set_xlabel('坐下开始后时间（秒，约10Hz只读采样）',fontproperties=font)
fig.suptitle('实机坐下过程：目标抖动先于最终稳定，不是两个模型轮流控制',fontproperties=font,fontsize=14)
fig.savefig(p/'sit-transition-targets.png',dpi=160)
summary='> **2026-10-10 新坐站实物过程已捕获（最新覆盖）：** 用户App一次坐下再站起完成，反馈坐下初期反复抖几次。50秒500次只读采样捕获walk→sitstand_sit约8.2秒→sitstand_stand约4秒→walk→停止保持，坐/站均正确种子SHA2a3edcb880…，sit指令始终[1,0,0]、stand始终0；无模型反复切换。实时短反馈寄存器67显示初期左膝目标约41.2°→−0.3°（约0.1秒间隔），随后多次反向，踝同步变化，约2秒后收敛。稳定坐姿实际两膝约+67.5/−69.1°、目标+67.5/−67.8°，稳定段关节负载较低；完整坐段7.4～8.3V、瞬态#14负载1000，无新故障卸力、发布状态无保持且IMU暂停计数0。用户确认已坐下，不能继续称坐不下/悬空；一次成功非稳定验收。直接现象是目标振荡，模型闭环/切换动作历史清零/仿真实机时序差异尚需隔离，未证实单一根因；本次只读无新控制参数/服务修改。状态约1Hz发布，不能逐帧与反馈对齐；曲线使用新鲜寄存器67而非缓存42。证据附件/平台/2026-10-10/bidirectional-sit-inspect/{timeline.json,calibration-capture.json,trajectory-analysis.json,sit-transition-targets.png}。\n\n'
for relative in ('AGENTS.md','docs/实测记录/README.md','docs/实测记录/03-舵机实测.md','docs/实测记录/04-网络与观测平台.md'):
    path=root/relative; text=path.read_text(encoding='utf-8')
    if summary not in text:path.write_text(summary+text,encoding='utf-8')
print('Target plot and latest hardware record saved.')
