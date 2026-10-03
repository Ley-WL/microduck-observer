"""Generate controlled variants of the existing, reviewed motion probe."""
from pathlib import Path

root = Path(__file__).resolve().parent
source = (root / 'test-small-oscillation-board.py').read_text(encoding='utf-8')
groups = {'one-leg': [12], 'one-head': [31], 'four-legs': [12, 14, 22, 24],
          'four-head': [30, 31, 32, 33], 'all14': None, 'readonly15': []}
for label, ids in groups.items():
    text = source.replace("ids=list(ALL_IDS[:-1]);", f"ids={repr(ids) if ids is not None else 'list(ALL_IDS[:-1])'};")
    text = text.replace("'mode':'all14-sine-1Hz-amplitude1degree-50Hz-10seconds'", f"'mode':'{label}-sine-1Hz-amplitude1degree-50Hz-10seconds'")
    if not ids and ids is not None:
        text = text.replace("starts={sid:rows[sid]['position'] for sid in ids}", "assert all(row['torque']==0 for row in rows.values()), '只读对照需要全卸力'\n    starts={sid:rows[sid]['position'] for sid in ids}")
    text = text.replace("        sync_write(bus,42,{sid:goal_bytes(value) for sid,value in targets.items()})", "        if targets: sync_write(bus,42,{sid:goal_bytes(value) for sid,value in targets.items()})")
    text = text.replace("    sync_write(bus,42,{sid:goal_bytes(starts[sid]) for sid in ids})", "    if ids: sync_write(bus,42,{sid:goal_bytes(starts[sid]) for sid in ids})")
    text = text.replace("small-oscillation-20261003.json", f"group-probe-{label}-20261003.json")
    (root / f'test-group-{label}-board.py').write_text(text, encoding='utf-8')

readonly = (root / 'test-group-readonly15-board.py').read_text(encoding='utf-8')
for label, read_ids in {'readonly1': [32], 'readonly1-12': [12], 'readonly1-23': [23], 'readonly4': [30, 31, 32, 33],
                        'readonly14-no32': [10,11,12,13,14,20,21,22,23,24,30,31,33,34]}.items():
    text = readonly.replace('def checked(rows):', 'def checked(rows, expected=ALL_IDS):')
    text = text.replace('assert set(rows)==set(ALL_IDS)', 'assert set(rows)==set(expected)')
    text = text.replace('sorted(set(ALL_IDS)-set(rows))', 'sorted(set(expected)-set(rows))')
    text = text.replace("        rows=bus.read_feedback_many(ALL_IDS)", f"        rows=bus.read_feedback_many({read_ids!r})")
    text = text.replace('        checked(rows)', f'        checked(rows, {read_ids!r})')
    text = text.replace('readonly15', label)
    (root / f'test-group-{label}-board.py').write_text(text, encoding='utf-8')
