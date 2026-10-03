from pathlib import Path
root=Path(__file__).resolve().parents[1]
for source,dest in [('deploy/install-servo-rx-resync.py','deploy/install-servo-official-io.py'),
                    ('tools/deploy-servo-rx-resync.py','tools/deploy-servo-official-io.py')]:
    text=(root/source).read_text(encoding='utf-8').replace('servo-rx-resync','servo-official-io').replace('current.rx-resync-next','current.official-io-next')
    if dest.startswith('deploy/'):
        text=text.replace('parser-only backend update','HD1910 I/O scheduling and feedback fallback update')
        text=text.replace("assert all(r['online'] and r['torque']==0 for r in before['joints']['data']['servos'])",
                          "assert all(r['online'] and r['torque'] in (0,1) for r in before['joints']['data']['servos'])\nregister_keys=('torque','goalPositionRaw','goalCurrentRaw','accelerationRaw','speedLimitRaw','torqueLimitRaw','kpRaw','kiRaw','kdRaw')\nregisters_before={str(r['id']):{k:r[k] for k in register_keys} for r in before['joints']['data']['servos']}")
        text=text.replace("assert all(r['torque']==0 for r in snapshot['joints']['data']['servos'] if r['online'])",
                          "registers_after={str(r['id']):{k:r[k] for k in register_keys} for r in snapshot['joints']['data']['servos'] if r['online']}\n    assert all(registers_before[sid]==row for sid,row in registers_after.items())\n    manifest.update({'servoRegistersBefore':registers_before,'servoRegistersAfter':registers_after})")
    (root/dest).write_text(text,encoding='utf-8')
