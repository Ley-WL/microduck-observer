"""Experimental settle-before-lift clock. Simulation first; not deployed."""
import torch
from rear_cycle_policy import CycleClock
class TransferClock(CycleClock):
 def __init__(self,n,device='cpu',dt=.02):
  super().__init__(n,device,dt);self.transfer_run=torch.zeros_like(self.stage);self.transfer_passed=torch.zeros(n,device=device,dtype=torch.bool)
 def step(self,pose_good,standing,transfer_good,dt=None):
  if dt is not None and dt<=0:return
  previous=self.progress.clone();oldstage=self.stage.clone();waiting=(oldstage==self.ENTER)&~self.transfer_passed&(previous>=.5-1e-5)
  self.transfer_run=torch.where(waiting,torch.where(transfer_good,self.transfer_run+1,0),self.transfer_run)
  self.transfer_passed|=waiting&(self.transfer_run>=15)
  super().step(pose_good,standing,dt)
  blocked=(oldstage==self.ENTER)&~self.transfer_passed&(self.progress>=.5)
  self.progress[blocked]=.5
 def pause(self):self.transfer_run.zero_();self.good_ticks.zero_();self.stand_run.zero_()
