import torch
from pose_entry_clock62 import PoseEntryClock

def test_hold_starts_after15_consecutive_ready_samples_then_lasts150_ticks():
 c=PoseEntryClock(1);c.trigger();c.progress[:]=1.;c.transfer_passed[:]=True;c.enter_limit=225;yes=torch.ones(1,dtype=torch.bool);no=~yes
 for _ in range(5):c.step(yes,no,yes)
 c.step(no,no,yes);assert c.entry_pose_run.item()==0
 for _ in range(14):c.step(yes,no,yes);assert c.stage.item()==c.ENTER
 c.step(yes,no,yes);assert c.stage.item()==c.HOLD and c.hold_ticks.item()==0
 for _ in range(149):c.step(yes,no,yes);assert c.stage.item()==c.HOLD
 c.step(yes,no,yes);assert c.stage.item()==c.EXIT and c.good_max.item()==150

def test_pause_partial_reset_and_cancel_during_pose_wait_preserve_continuity():
 c=PoseEntryClock(2);c.trigger();c.progress[:]=1.;c.transfer_passed[:]=True;c.enter_limit=225;yes=torch.ones(2,dtype=torch.bool);no=~yes
 for _ in range(10):c.step(yes,no,yes)
 c.step(yes,no,yes,dt=0);assert (c.entry_pose_run==10).all()
 c.reset(torch.tensor([0]));assert c.entry_pose_run.tolist()==[0,10]
 c.pause();assert not c.entry_pose_run.any();c.cancel(torch.tensor([1]));before=c.progress[1].item();c.step(yes,no,yes);assert c.progress[1]<before and c.stage[1]==c.EXIT
 c=PoseEntryClock(1);c.trigger();c.enter_limit=225
 for _ in range(224):c.step(torch.tensor([False]),torch.tensor([False]),torch.tensor([True]))
 assert c.stage.item()==c.ENTER
 c.step(torch.tensor([False]),torch.tensor([False]),torch.tensor([True]));assert c.stage.item()==c.EXIT and c.aborted.item()
