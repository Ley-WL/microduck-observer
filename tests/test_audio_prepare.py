import importlib.util
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location('audio',Path(__file__).parents[1]/'deploy/audio_prepare.py')
a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
class AudioTests(unittest.TestCase):
 def test_reject_wrong_kernel_before_install(self):
  with self.assertRaises(RuntimeError):a.verify_assets(Path('/missing'),'other')
 def test_validated_modules_and_vermagic(self):
  root=Path(__file__).parents[1]/'deploy/hardware/audio'
  a.verify_assets(root,a.KERNEL,lambda *args:a.KERNEL+' SMP preempt')
  with self.assertRaises(RuntimeError):a.verify_assets(root,a.KERNEL,lambda *args:'wrong kernel')
 def test_restarts_preserve_user_volume(self):
  plan=a.mixer_plan(['Left Line Mixer PGAL Bypass'],False)
  self.assertIn(('Left Line Mixer PGAL Bypass','off'),plan)
  self.assertNotIn(('PCM','0dB'),plan)
  self.assertIn(('PCM','0dB'),a.mixer_plan([],True))
 def test_late_alsa_devices_and_order(self):
  self.assertIn('DeviceAllow=char-alsa rw',a.AUDIO_DROP)
  self.assertIn('After=microduck-audio-prepare.service',a.AUDIO_DROP)
  self.assertIn('TimeoutStartSec=20',a.UNIT)
if __name__=='__main__':unittest.main()
