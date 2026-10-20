from pathlib import Path
import re,json,hashlib
import sys
if len(sys.argv)!=3:raise SystemExit('Usage: check_geometry.py ORIGINAL_VENDOR_C PATCHED_C')
s=Path(sys.argv[1]).read_text();t=Path(sys.argv[2]).read_text()
for name in ['imx219_init_tab_1920_1080_30fps','imx219_init_tab_3280_2464_21fps']:
 pattern=r'static const struct imx219_reg '+name+r'\[\] = \{.*?\n\};'
 assert re.search(pattern,s,re.S).group()==re.search(pattern,t,re.S).group()
v=t.split('static const struct imx219_reg imx219_vga_regs[] = {')[1].split('\n};')[0]
r={int(a,16):int(b,16) for a,b in re.findall(r'\{(0x[0-9a-f]+), (0x[0-9a-f]+)\}',v)}
w=lambda a:(r[a]<<8)|r[a+1]
assert w(0x166)-w(0x164)+1==1280
assert w(0x16a)-w(0x168)+1==960
assert (w(0x16c),w(0x16e))==(640,480)
assert r[0x174]==r[0x175]==3
assert r[0x18c]==r[0x18d]==10
assert w(0x160)==0x6e6
assert 'if (priv->cur_mode->reg_list != imx219_vga_regs)' in t
print(json.dumps({'geometry_check':'passed','existing_register_tables':'unchanged','input_roi':[1000,752,1280,960],'sensor_output':[640,480],'binning_registers':[3,3],'raw_bits':10,'hardware_capture':'NOT_TESTED'},indent=2))
