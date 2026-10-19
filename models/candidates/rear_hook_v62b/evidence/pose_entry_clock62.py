"""Require consecutive pose readiness before beginning the unchanged3s hold."""
import torch
from rear_transfer_gate41 import TransferClock

class PoseEntryClock(TransferClock):
 def __init__(self,n,device='cpu',dt=.02):
  super().__init__(n,device,dt);self.entry_pose_run=torch.zeros_like(self.stage);self.entry_pose_required=15
 def step(self,pose_good,standing,transfer_good,dt=None):
  if dt is not None and dt<=0:return
  entering=self.stage==self.ENTER;at_pose=entering&(self.progress>=1-1e-5)
  self.entry_pose_run=torch.where(at_pose,torch.where(pose_good,self.entry_pose_run+1,0),torch.zeros_like(self.entry_pose_run))
  effective_pose=torch.where(entering,pose_good&(self.entry_pose_run>=self.entry_pose_required),pose_good)
  super().step(effective_pose,standing,transfer_good,dt)
 def pause(self):
  super().pause();self.entry_pose_run.zero_()
