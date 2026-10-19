"""Root-owned, bounded HAT audio preparation; never opens servo ports or plays audio."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

KERNEL = '6.1.115-vendor-rk35xx'
HASHES = {
    'snd-soc-tlv320aic3x.ko': 'f1c07a1c551196bdf94d732871b604a821b732089e003ec074606316b462b86c',
    'snd-soc-tlv320aic3x-i2c.ko': '2f0241212703d84ea26b6711e511b81d24b09b412d7498a9d2c4b57c0b2c6c6f',
}
AUDIO_DROP = '[Unit]\nWants=microduck-audio-prepare.service\nAfter=microduck-audio-prepare.service\n[Service]\nSupplementaryGroups=audio\nDeviceAllow=char-alsa rw\n'
UNIT = '[Unit]\nDescription=Prepare MicroDuck HAT audio without robot motion\nAfter=systemd-modules-load.service alsa-restore.service\nBefore=microduck-observer.service\n[Service]\nType=oneshot\nExecStart=/usr/bin/python3 /usr/local/lib/microduck-services/audio_prepare.py\nTimeoutStartSec=20\n'

def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, timeout=5)

def verify_assets(source, kernel, runner=run):
    if kernel != KERNEL:
        raise RuntimeError('Unsupported audio module kernel: ' + kernel)
    for name, digest in HASHES.items():
        p = source / name
        if hashlib.sha256(p.read_bytes()).hexdigest() != digest:
            raise RuntimeError('Audio driver checksum mismatch: ' + name)
        if not runner('modinfo', '-F', 'vermagic', str(p)).startswith(kernel + ' '):
            raise RuntimeError('Audio driver vermagic mismatch: ' + name)

def install(source):
    kernel = run('uname', '-r').strip()
    verify_assets(source, kernel)
    target = Path('/lib/modules') / kernel / 'updates/dkms'
    target.mkdir(parents=True, exist_ok=True)
    for name, digest in HASHES.items():
        dest = target / name
        if dest.exists() and hashlib.sha256(dest.read_bytes()).hexdigest() != digest:
            raise RuntimeError('Different existing audio module: ' + str(dest))
        shutil.copyfile(source / name, dest); dest.chmod(0o644)
    # First install indexes the entire vendor kernel on an SD card.
    subprocess.check_output(['depmod', '-a', kernel], text=True,
                            stderr=subprocess.STDOUT, timeout=120)
    helper = Path('/usr/local/lib/microduck-services/audio_prepare.py')
    helper.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(__file__, helper); helper.chmod(0o644)
    Path('/etc/modules-load.d/microduck-hat-audio.conf').write_text('snd_soc_tlv320aic3x_i2c\n')
    Path('/etc/systemd/system/microduck-audio-prepare.service').write_text(UNIT)
    drop = Path('/etc/systemd/system/microduck-observer.service.d/audio.conf')
    drop.parent.mkdir(parents=True, exist_ok=True); drop.write_text(AUDIO_DROP)

def mixer_plan(controls, initialize):
    changes = [(n, 'off') for n in controls if 'Mixer' in n and 'Bypass' in n]
    changes += [('Right Line Mixer DACR1', 'on')]
    if initialize:
        changes += [('PCM', '0dB'), ('Line DAC', '0dB'), ('Line', '0dB,unmute')]
    return changes

def prepare():
    status = {'ready': False, 'motionSent': False}
    Path('/run/microduck-audio.json').write_text(json.dumps(status))
    try:
        run('modprobe', 'snd-soc-tlv320aic3x-i2c')
        for _ in range(20):
            if Path('/dev/snd/controlC1').exists(): break
            time.sleep(.1)
        if 'aic3104' not in Path('/proc/asound/cards').read_text():
            raise RuntimeError('AIC3104 not registered; check HAT/overlay/power')
        marker = Path('/var/lib/microduck-audio/initialized')
        controls = [line.split("'")[1] for line in run('amixer','-c','aic3104','scontrols').splitlines()]
        for name, value in mixer_plan(controls, not marker.exists()):
            run('amixer','-q','-c','aic3104','--','sset',name,value)
        run('alsactl','store','1')
        marker.parent.mkdir(parents=True,exist_ok=True); marker.touch()
        status['ready'] = True
    except Exception as exc:
        status['error'] = str(exc)
    # A disconnected HAT must not block the robot service or cause reboot loops.
    Path('/run/microduck-audio.json').write_text(json.dumps(status))
    print(json.dumps(status), flush=True)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('--install', type=Path)
    args = parser.parse_args()
    if os.geteuid() != 0: raise SystemExit('Requires root')
    if args.install: install(args.install)
    else: prepare()
