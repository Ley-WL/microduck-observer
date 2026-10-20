from pathlib import Path
import hashlib,json,re,zipfile,csv
module=Path(__file__).resolve().parents[1];observer=module.parents[1];workspace=observer.parent;version=module/'revN-eco1'
record='''> **2026-10-20 硬件资料归档位置更新（用户要求，文件整理）：** 已在Observer建立 `hardware/`，今后硬件设计资料按模块/版本存放。电源Hub当前归档为 `hardware/power-hub/revN-eco1/`：无开关，C225336×3+C1去耦，Ø3mm输入；收录ECO1立创工程、源KiCad、仅9加工文件的Gerber ZIP、两行BOM/候选料清单、效果图、厂商资料、审计/用户反馈和SHA256来源清单，并提供完整资料ZIP。复制字节/哈希及各ZIP完整性核对通过；旧RL文件未删，目录入口/AGENTS已同步。只是整理，不增加验证结论：源ERC/DRC通过，立创新预览/DRC、孔距/线序/绝缘/带载温升仍待确认；C1料号仍为可替代候选，未采购、打板或访问设备。当前从 [Observer硬件目录](../../hardware/README.md) 或 [电源Hub入口](../../hardware/power-hub/README.md) 继续；实测主记录仍留本目录。

'''
for name in ['07-HAT实测.md','README.md']:
 f=observer/'docs/实测记录'/name;s=f.read_text(encoding='utf-8-sig')
 if not s.startswith('> **2026-10-20 硬件资料归档位置更新'):f.write_text(record+s,encoding='utf-8')
old=workspace/'microduck-rl/hardware/power-hub'
f=old/'README.md';s=f.read_text(encoding='utf-8-sig');head='> **硬件正式维护位置已迁至Observer。** 当前版本为 [Observer hardware/power-hub Rev N / ECO1](../../../microduck-observer/hardware/power-hub/README.md)。本目录保留历史来源，后续正式版本放在 `microduck-observer/hardware/`。\n\n'
if not s.startswith('> **硬件正式维护位置'):f.write_text(head+s,encoding='utf-8')
f=old/'revN-passive-cap/README.md';s=f.read_text(encoding='utf-8-sig');head='> **当前整理版以 [Observer硬件目录](../../../../microduck-observer/hardware/power-hub/revN-eco1/README.md) 为入口。** 此目录保留历史生成过程；后续修改从Observer hardware继续。\n\n'
if not s.startswith('> **当前整理版'):f.write_text(head+s,encoding='utf-8')
# Read-only integrity and local documentation-link verification after organization.
manifest=json.loads((version/'manifest.json').read_text(encoding='utf-8'))
for name,digest in manifest['files'].items():assert hashlib.sha256((version/name).read_bytes()).hexdigest()==digest,name
for archive in [version/'eda/MicroDuck-Hub-RevN-Cap-ECO1.epro',version/'manufacturing/MicroDuck-Hub-RevN-Gerber.zip',module/'packages/MicroDuck-Hub-RevN-ECO1-Complete.zip']:
 with zipfile.ZipFile(archive) as z:assert z.testzip() is None
with zipfile.ZipFile(version/'manufacturing/MicroDuck-Hub-RevN-Gerber.zip') as z:assert len(z.namelist())==9
with (version/'bom/BOM.csv').open(encoding='utf-8-sig') as f:
 rows=list(csv.DictReader(f));assert len(rows)==2 and sum(int(r['每板数量']) for r in rows)==4
for f in [observer/'hardware/README.md',module/'README.md',version/'README.md']:
 for target in re.findall(r'\]\(([^)]+)\)',f.read_text(encoding='utf-8')):
  if not target.startswith(('http:','https:','#')):assert (f.parent/target.split('#')[0]).exists(),(f,target)
print('Verified:40 version files,all recorded SHA256s,3 ZIPs,9 Gerber/drill files,2 BOM rows/4 parts,all entry links.')
