"""Export official MuJoCo keyframes for a read-only pose viewer; no hardware IO."""
import json
from pathlib import Path
import mujoco
import numpy as np
import fast_simplification

root = Path(__file__).resolve().parents[6]
scene = root / 'microduck-rl/src/mjlab_microduck/robot/microduck/scene.xml'
model = mujoco.MjModel.from_xml_path(str(scene))
data = mujoco.MjData(model)
gids = [g for g in range(model.ngeom) if model.geom_group[g] == 2 and model.geom_type[g] == mujoco.mjtGeom.mjGEOM_MESH]
meshes = {}
for mid in sorted(set(int(model.geom_dataid[g]) for g in gids)):
    v = model.mesh_vert[model.mesh_vertadr[mid]:model.mesh_vertadr[mid]+model.mesh_vertnum[mid]].copy()
    f = model.mesh_face[model.mesh_faceadr[mid]:model.mesh_faceadr[mid]+model.mesh_facenum[mid]].copy()
    v, remap = np.unique(np.round(v, 7), axis=0, return_inverse=True)
    f = remap[f]
    # Quadric decimation keeps CAD silhouettes and planar surfaces compact.
    nv, nf = fast_simplification.simplify(v.astype(float) * 1000, f, target_count=min(len(f), 650), agg=10)
    nv /= 1000
    used, inverse = np.unique(nf, return_inverse=True)
    nv, nf = nv[used], inverse.reshape(-1, 3)
    if len(nf) > 1000:
        for cells in range(24, 3, -1):
            scale = max(np.ptp(v, axis=0)) / cells
            keys = np.round(v / scale).astype(int)
            unique, inv = np.unique(keys, axis=0, return_inverse=True)
            nv = np.zeros((len(unique), 3)); counts = np.bincount(inv)
            for axis in range(3): nv[:, axis] = np.bincount(inv, weights=v[:, axis]) / counts
            nf = inv[f]
            nf = nf[(nf[:,0] != nf[:,1]) & (nf[:,1] != nf[:,2]) & (nf[:,0] != nf[:,2])]
            _, idx = np.unique(np.sort(nf, axis=1), axis=0, return_index=True)
            nf = nf[idx]
            if len(nf) <= 1000: break
    meshes[str(mid)] = dict(vertices=np.round(nv, 6).reshape(-1).tolist(), faces=nf.reshape(-1).tolist())
poses = {}
for name in ['SIT', 'STAND']:
    kid = mujoco.mj_name2id(model, mujoco.mjtObj.mjOBJ_KEY, name)
    mujoco.mj_resetDataKeyframe(model, data, kid)
    mujoco.mj_forward(model, data)
    poses[name] = dict(qpos=np.round(data.qpos, 7).tolist(), geoms=[dict(mesh=int(model.geom_dataid[g]), pos=np.round(data.geom_xpos[g], 7).tolist(), matrix=np.round(data.geom_xmat[g],7).tolist()) for g in gids])
out = dict(source=str(scene), commit='cfe1c2adcceb55f6b6e369c888b31c6873175c55', kind='MuJoCo forward kinematics, not a policy rollout', meshes=meshes, poses=poses)
dest = Path(__file__).with_name('official-poses.json')
dest.write_text(json.dumps(out, separators=(',',':')), encoding='utf-8')
print(json.dumps(dict(file=str(dest), bytes=dest.stat().st_size, visualGeoms=len(gids), nq=model.nq, joints=[mujoco.mj_id2name(model,mujoco.mjtObj.mjOBJ_JOINT,j) for j in range(model.njnt)], sit=poses['SIT']['qpos'], stand=poses['STAND']['qpos'])))
