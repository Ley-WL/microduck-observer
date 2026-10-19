"""Simulation-only sensor hypotheses. Window fit excludes manual holdout trial."""
import torch

class VelocityReporter:
 def __init__(self,profile,dt=.02):
  if profile not in ['legacy20ms','delay40ms','window56']:raise ValueError(profile)
  self.profile=profile;self.dt=dt;self.velocity_history=None;self.position_history=None
 def sample(self,q,velocity):
  if self.velocity_history is None:
   self.velocity_history=torch.stack([velocity.clone(),velocity.clone()])
   self.position_history=torch.stack([q-velocity*(k*self.dt) for k in range(1,5)])
  if self.profile=='legacy20ms':observed=self.velocity_history[0]
  elif self.profile=='delay40ms':observed=self.velocity_history[1]
  else:
   # Total observed reporter fit:10ms delay plus60ms difference window.
   # At50Hz this uses linearly interpolated positions at ages.5 and3.5 ticks.
   history=torch.cat([q[None],self.position_history],0)
   observed=(.5*(history[0]+history[1])-.5*(history[3]+history[4]))/.06
  self.velocity_history=torch.stack([velocity.clone(),self.velocity_history[0]])
  self.position_history=torch.cat([q[None].clone(),self.position_history[:3]],0)
  # Preserve the exact floating-point operation order of native evaluator41.
  return (observed*4096/(torch.pi*2)/50).round()*50*(torch.pi*2)/4096
