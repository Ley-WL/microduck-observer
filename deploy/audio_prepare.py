"""Root-owned, bounded HAT audio preparation; never opens servo ports or plays audio."""
import argparse
import hashlib
import json
import os
import re
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

def mixer_state(output):
    """Parse raw ALSA controls, avoiding ambiguous simple-mixer stereo writes."""
    result = {}
    for block in re.split(r"(?=^numid=)", output, flags=re.M):
        header = re.search(r"^numid=(\d+),.*name='([^']+)'", block)
        kind = re.search(r"type=(BOOLEAN|INTEGER),", block)
        values = re.search(r"^\s*: values=(.*)$", block, re.M)
        if not header or not kind or not values:
            continue
        name = header[2]
        tokens = values[1].strip().split(',')
        if kind[1] == 'BOOLEAN':
            if any(v not in ('on', 'off') for v in tokens):
                raise RuntimeError('Invalid switch readback: ' + name)
            parsed = [v == 'on' for v in tokens]
        else:
            low = re.search(r'min=(-?\d+),max=(-?\d+)', block)
            if not low:
                raise RuntimeError('Missing control bounds: ' + name)
            parsed = [int(v) for v in tokens]
            if any(not int(low[1]) <= v <= int(low[2]) for v in parsed):
                raise RuntimeError('Out-of-range mixer readback: ' + name)
        count = re.search(r'values=(\d+)', block)
        if not count or len(parsed) != int(count[1]):
            raise RuntimeError('Incomplete mixer readback: ' + name)
        result[name] = {'id': header[1], 'type': kind[1], 'values': parsed}
    return result


PLAYBACK_LIMITS = {'PCM Playback Volume': 127, 'Line DAC Playback Volume': 118,
                   'Line Playback Volume': 9}
LEVELS = Path('/var/lib/microduck-audio/playback-levels.json')


def validate_levels(levels):
    if not isinstance(levels, dict) or set(levels) != set(PLAYBACK_LIMITS):
        raise RuntimeError('Invalid saved audio volume controls')
    for name, maximum in PLAYBACK_LIMITS.items():
        values = levels[name]
        if not isinstance(values, list) or len(values) != 2 or any(
                type(v) is not int or not 0 <= v <= maximum for v in values):
            raise RuntimeError('Invalid saved audio volume: ' + name)
    return levels


def load_levels(path):
    if not path.exists():
        return None
    document = json.loads(path.read_text())
    if document.get('schemaVersion') != 1:
        raise RuntimeError('Invalid saved audio volume schema')
    return validate_levels(document.get('playbackVolumes'))


def save_levels(path, levels):
    validate_levels(levels)
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix('.tmp')
    with temp.open('w') as output:
        json.dump({'schemaVersion': 1, 'playbackVolumes': levels}, output)
        output.flush()
        os.fsync(output.fileno())
    os.chmod(temp, 0o600)
    os.replace(temp, path)
    directory = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def configure_mixer(initialize, runner=run, saved_levels=None):
    before = mixer_state(runner('amixer', '-c', 'aic3104', 'contents'))
    required = {'Line Playback Switch': True,
                'Right Line Mixer DACR1 Switch': True}
    required.update({n: False for n, c in before.items()
                     if c['type'] == 'BOOLEAN' and 'Mixer' in n and 'Bypass' in n})
    expected = {}
    for name, value in required.items():
        if name not in before or before[name]['type'] != 'BOOLEAN':
            raise RuntimeError('Missing output switch: ' + name)
        expected[name] = [value] * len(before[name]['values'])
    volumes = ['PCM Playback Volume', 'Line DAC Playback Volume', 'Line Playback Volume']
    for name in volumes:
        if name not in before or before[name]['type'] != 'INTEGER':
            raise RuntimeError('Missing playback volume: ' + name)
    preserved = {n: c['values'] for n, c in before.items()
                 if c['type'] == 'INTEGER' and 'Playback Volume' in n}
    if initialize:
        for name, value in zip(volumes, [127, 118, 0]):
            expected[name] = [value] * len(before[name]['values'])
    if saved_levels is not None:
        expected.update(validate_levels(saved_levels))
    repaired = []
    for name, values in expected.items():
        if before[name]['values'] == values:
            continue
        raw = ','.join(('on' if v else 'off') if isinstance(v, bool) else str(v)
                       for v in values)
        runner('amixer', '-q', '-c', 'aic3104', 'cset',
               'numid=' + before[name]['id'], raw)
        repaired.append(name)
    after = mixer_state(runner('amixer', '-c', 'aic3104', 'contents'))
    for name, values in {**preserved, **expected}.items():
        if name not in after or after[name]['values'] != values:
            raise RuntimeError('Audio readback verification failed: ' + name)
    return {'outputVerified': True, 'volumePreserved': not initialize,
            'volumeReference': 'saved-target' if saved_levels is not None else 'initial-readback',
            'savedVolumeRestored': saved_levels is not None and any(n in repaired for n in volumes),
            'repairedControls': repaired,
            'playbackVolumes': {n: after[n]['values'] for n in volumes}}


def prepare():
    status = {'ready': False, 'motionSent': False, 'outputVerified': False}
    Path('/run/microduck-audio.json').write_text(json.dumps(status))
    try:
        run('modprobe', 'snd-soc-tlv320aic3x-i2c')
        for _ in range(20):
            if Path('/dev/snd/controlC1').exists(): break
            time.sleep(.1)
        if 'aic3104' not in Path('/proc/asound/cards').read_text():
            raise RuntimeError('AIC3104 not registered; check HAT/overlay/power')
        marker = Path('/var/lib/microduck-audio/initialized')
        saved_levels = load_levels(LEVELS)
        status.update(configure_mixer(not marker.exists(), saved_levels=saved_levels))
        run('alsactl','store','1')
        if saved_levels is None:
            save_levels(LEVELS, status['playbackVolumes'])
        marker.parent.mkdir(parents=True,exist_ok=True); marker.touch()
        status['ready'] = True
    except Exception as exc:
        status['outputVerified'] = False
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
