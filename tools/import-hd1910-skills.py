"""Replace platform recovery/sitstand weights with the audited replica models."""
import hashlib
import json
from pathlib import Path
import shutil

root = Path(__file__).resolve().parents[1]
source = root.parent / 'microduck-rl/artifacts/hd1910-skills-20261009/qualified-models'
audit = json.loads((source / 'evidence/qualification-audit.json').read_text(encoding='utf-8'))
proof = json.loads((source / 'evidence/server-provenance-audit.json').read_text(encoding='utf-8'))
assert audit['goalAchieved'] and all(row['qualified'] for row in audit['candidates'].values())
backup = root / 'deploy/staging/hd1910-skills-20261010/previous-models'
backup.mkdir(parents=True, exist_ok=True)
for kind, name in [('getup', 'xgoduck_getup'), ('sitstand', 'alpha_sitstand')]:
    model = source / 'models' / f'{kind}.onnx'
    metadata = json.loads((source / 'models' / f'{kind}.metadata.json').read_text(encoding='utf-8'))
    digest = hashlib.sha256(model.read_bytes()).hexdigest()
    assert digest == metadata['policySha256'] == audit['candidates'][kind]['policySha256']
    assert metadata['skillKind'] == kind and metadata['controlHz'] == 50
    assert metadata['actorObservationDim'] == 61 and metadata['actionDim'] == 14
    for scope in audit['candidates'][kind]['scopes'].values():
        assert scope['complete'] and scope['accepted'] and all(group['rate'] >= .95 for group in scope['groups'])
    metadata.update(runtimeKind=f'hd1910_{kind}', runtimeActionAlpha=.45,
                    source='microduck-rl replica/HD1910 retraining 2026-10-10',
                    geometry='Updated replica STL/XML; 68 body-floor meshes, two feet and two inherited leg contact meshes',
                    training={'run': proof[kind]['run'], 'checkpoint': proof[kind]['checkpoint'],
                              'checkpointSha256': proof[kind]['checkpointSha256']},
                    simulationAcceptance={scope: {'worstGroupSuccessRate': value['worstRate'],
                                                   'scenarios': len(value['groups']) // 2,
                                                   'trialsPerScenarioPerSeed': 200, 'seeds': [1009,2027]}
                                          for scope,value in audit['candidates'][kind]['scopes'].items()},
                    transferBoundary='Simulation accepted, hardware motion not validated. Shared platform calibration and HD1910 temporary gains retained.')
    if kind == 'sitstand':
        metadata.update(commandEncoding={'slot':'twist.vx','sit':1,'stand':0},
                        sitSettleSeconds=4, standSeconds=4, uprightHoldSeconds=1)
    else:
        metadata.update(getupMinSeconds=3, uprightHoldSeconds=1)
    for suffix in ['onnx', 'metadata.json']:
        old = root / 'models' / f'{name}.{suffix}'
        preserved = backup / old.name
        if old.exists() and not preserved.exists():
            shutil.copy2(old, preserved)
    shutil.copy2(model, root / 'models' / f'{name}.onnx')
    (root / 'models' / f'{name}.metadata.json').write_text(json.dumps(metadata,indent=2), encoding='utf-8')
    print(f'{name}: {digest}, {metadata["runtimeKind"]}')
