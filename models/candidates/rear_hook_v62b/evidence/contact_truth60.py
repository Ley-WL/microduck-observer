"""Classify actual foot shells as feet; keep every physical collision unchanged.

Canonical sole contacts still determine support/standing. Full-foot clearance
also includes each physical foot shell, so the new lift test is stricter.
"""
from dataclasses import replace
import numpy as np,torch
from scipy.spatial import ConvexHull
from mjlab.managers import RewardTermCfg
from mjlab.utils.lab_api.math import matrix_from_quat
from rear_encoder_env_v32 import make_cfg as original_cfg,get_encoder_spec
from rear_cycle_env import RearCycleReward,measurements as sole_measurements,smooth
from mjlab_microduck.tasks import mdp

def foot_shell_mapping():
 model=get_encoder_spec().compile();mapping={}
 for i in range(model.ngeom):
  g=model.geom(i)
  if not g.name.startswith('body_ground_'):continue
  mesh=model.mesh(int(model.geom_dataid[i])).name
  if mesh in ['foot_left','foot_right']:
   side=mesh.split('_')[1];assert side not in mapping
   sole=model.geom(side+'_foot_collision').id
   assert model.geom_bodyid[i]==model.geom_bodyid[sole],(mesh,'not a rigid foot part')
   mapping[side]=dict(name=g.name,mesh=mesh,body=model.body(int(model.geom_bodyid[i])).name,geom=i)
 assert set(mapping)=={'left','right'},mapping
 return model,mapping

class WholeFootReward(RearCycleReward):
 def __init__(self,cfg,env):
  super().__init__(cfg,env);model,mapping=foot_shell_mapping();self.visual_geometry=[]
  for side in ['left','right']:
   name=mapping[side]['name'];geom=mapping[side]['geom'];mesh=model.geom_dataid[geom];start,count=model.mesh_vertadr[mesh],model.mesh_vertnum[mesh];v=np.unique(model.mesh_vert[start:start+count],axis=0);v=v[ConvexHull(v).vertices]
   ids,_=env.scene['robot'].find_geoms([name]);self.visual_geometry.append((ids[0],torch.tensor(v.copy(),device=env.device)))
 def __call__(self,env):
  c=env.command_manager.get_term('twist');m=measurements(env);p=c.progress;lift=smooth(2*p-1);transfer=smooth(2*p)
  tracking=mdp.hd1910_single_leg_tracking.__call__(self,env,height_std=.012)
  rear_score=torch.exp(-((m['rear']-.075*lift)/.025).square());task=12*tracking*(.25+.75*rear_score)
  task+=8*m['pose'].float()*(p>.99)+8*m['stand'].float()*(1-transfer)-2*(1-transfer)*m['head']
  return task+5/env.step_dt*c.clock.just_success.float()-3/env.step_dt*c.clock.just_abort.float()

def measurements(env):
 m=sole_measurements(env);r=env.scene['robot'];geometry=env.reward_manager.get_term_cfg('rear_cycle').func.visual_geometry;heights=[]
 for geom,vertices in geometry:
  rot=matrix_from_quat(r.data.geom_quat_w[:,geom]);world=vertices[None]@rot.transpose(-1,-2)+r.data.geom_pos_w[:,geom,None];heights.append(world[:,:,2].amin(-1)-env.scene.terrain.env_origins[:,2])
 m['sole_h']=m['h'];m['visual_h']=torch.stack(heights,-1);m['h']=torch.minimum(m['sole_h'],m['visual_h']);m['pose'] &= m['h'][:,1]>.020
 return m

def make_cfg(worlds,play=True):
 cfg=original_cfg(worlds,play=play);_,mapping=foot_shell_mapping();exclude=tuple(mapping[side]['name'] for side in ['left','right']);sensors=[]
 for sensor in cfg.scene.sensors:
  if sensor.name=='single_leg_body_ground':sensor=replace(sensor,primary=replace(sensor.primary,exclude=tuple(dict.fromkeys([*(sensor.primary.exclude or ()),*exclude]))))
  sensors.append(sensor)
 cfg.scene.sensors=tuple(sensors);cfg.rewards['rear_cycle']=RewardTermCfg(func=WholeFootReward,weight=1.)
 return cfg
