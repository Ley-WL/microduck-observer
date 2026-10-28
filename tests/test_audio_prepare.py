import importlib.util
from pathlib import Path
import unittest
import tempfile
spec=importlib.util.spec_from_file_location('audio',Path(__file__).parents[1]/'deploy/audio_prepare.py')
a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
class AudioTests(unittest.TestCase):
 def test_reject_wrong_kernel_before_install(self):
  with self.assertRaises(RuntimeError):a.verify_assets(Path('/missing'),'other')
 def test_validated_modules_and_vermagic(self):
  root=Path(__file__).parents[1]/'deploy/hardware/audio'
  a.verify_assets(root,a.KERNEL,lambda *args:a.KERNEL+' SMP preempt')
  with self.assertRaises(RuntimeError):a.verify_assets(root,a.KERNEL,lambda *args:'wrong kernel')
 def mixer(self, muted=False, bypass=False):
  values={'PCM Playback Volume':('INTEGER',[97,109],0,127),
          'Line DAC Playback Volume':('INTEGER',[100,118],0,118),
          'Line Playback Volume':('INTEGER',[2,3],0,9),
          'Line Playback Switch':('BOOLEAN',[not muted,not muted],0,1),
          'Right Line Mixer DACR1 Switch':('BOOLEAN',[not muted],0,1),
          'Right HP Mixer Line2L Bypass Switch':('BOOLEAN',[bypass],0,1),
          'Right HP Mixer Line2L Bypass Volume':('INTEGER',[6],0,118)}
  calls=[]
  def runner(*args):
   calls.append(args)
   if args[-1]=='contents':
    out=[]
    for i,(name,(kind,v,low,high)) in enumerate(values.items(),1):
     raw=','.join(('on' if x else 'off') if kind=='BOOLEAN' else str(x) for x in v)
     out.append(f"numid={i},iface=MIXER,name='{name}'\n ; type={kind},access=rw,values={len(v)},min={low},max={high}\n : values={raw}\n")
    return ''.join(out)
   assert args[4]=='cset',args
   name=list(values)[int(args[5].split('=')[1])-1]
   kind,old,low,high=values[name]
   v=[x=='on' for x in args[6].split(',')] if kind=='BOOLEAN' else [int(x) for x in args[6].split(',')]
   values[name]=(kind,v,low,high)
   return ''
  return values,calls,runner
 def test_restart_repairs_mute_and_bypass_preserving_stereo_volume(self):
  values,calls,runner=self.mixer(muted=True,bypass=True)
  before={n:list(v[1]) for n,v in values.items() if v[0]=='INTEGER'}
  result=a.configure_mixer(False,runner)
  self.assertTrue(result['outputVerified']);self.assertTrue(result['volumePreserved'])
  self.assertEqual(values['Line Playback Switch'][1],[True,True])
  self.assertEqual(values['Right HP Mixer Line2L Bypass Switch'][1],[False])
  self.assertEqual({n:v[1] for n,v in values.items() if v[0]=='INTEGER'},before)
 def test_healthy_restart_does_not_write_controls(self):
  values,calls,runner=self.mixer()
  self.assertEqual(a.configure_mixer(False,runner)['repairedControls'],[])
  self.assertEqual(len(calls),2)
 def test_first_init_sets_both_channels_to_baseline(self):
  values,calls,runner=self.mixer(muted=True)
  a.configure_mixer(True,runner)
  self.assertEqual(values['PCM Playback Volume'][1],[127,127])
  self.assertEqual(values['Line DAC Playback Volume'][1],[118,118])
  self.assertEqual(values['Line Playback Volume'][1],[0,0])
 def test_successful_write_with_wrong_readback_fails(self):
  values,calls,runner=self.mixer(muted=True)
  def ignored(*args):
   return runner(*args) if args[-1]=='contents' else ''
  with self.assertRaisesRegex(RuntimeError,'readback verification'):
   a.configure_mixer(False,ignored)
 def test_i2c_error_and_missing_controls_are_not_ready(self):
  with self.assertRaisesRegex(RuntimeError,'Missing output switch'):
   a.configure_mixer(False,lambda *args:'')
  def failure(*args):raise TimeoutError('I2C timeout')
  with self.assertRaises(TimeoutError):a.configure_mixer(False,failure)
 def test_corrupt_or_partial_mixer_readback_is_rejected(self):
  with self.assertRaisesRegex(RuntimeError,'Out-of-range'):
   a.mixer_state("numid=1,iface=MIXER,name='Line Playback Volume'\n ; type=INTEGER,values=2,min=0,max=9\n : values=15,15\n")
  with self.assertRaisesRegex(RuntimeError,'Incomplete'):
   a.mixer_state("numid=1,iface=MIXER,name='Line Playback Switch'\n ; type=BOOLEAN,values=2\n : values=on\n")
 def test_legal_fault_volume_is_restored_from_saved_reference(self):
  values,calls,runner=self.mixer(muted=True)
  values['PCM Playback Volume']=('INTEGER',[15,15],0,127)
  target={'PCM Playback Volume':[127,127],'Line DAC Playback Volume':[118,118],'Line Playback Volume':[0,0]}
  result=a.configure_mixer(False,runner,saved_levels=target)
  self.assertTrue(result['savedVolumeRestored'])
  self.assertEqual(values['PCM Playback Volume'][1],[127,127])
  self.assertEqual(values['Line Playback Switch'][1],[True,True])
 def test_saved_custom_volume_is_preserved_instead_of_forcing_zero_db(self):
  values,calls,runner=self.mixer()
  target={'PCM Playback Volume':[90,100],'Line DAC Playback Volume':[110,105],'Line Playback Volume':[1,2]}
  result=a.configure_mixer(False,runner,saved_levels=target)
  self.assertEqual(result['playbackVolumes'],target)
  self.assertEqual(a.configure_mixer(False,runner,saved_levels=target)['repairedControls'],[])
 def test_invalid_saved_volume_never_writes_mixer(self):
  values,calls,runner=self.mixer()
  with self.assertRaisesRegex(RuntimeError,'Invalid saved'):
   a.configure_mixer(False,runner,saved_levels={'PCM Playback Volume':[999,999]})
  self.assertTrue(all(c[-1]=='contents' for c in calls))
 def test_saved_reference_survives_reload_and_bad_input_keeps_old_file(self):
  target={'PCM Playback Volume':[127,109],'Line DAC Playback Volume':[100,118],'Line Playback Volume':[0,0]}
  with tempfile.TemporaryDirectory() as folder:
   path=Path(folder)/'levels.json';self.assertIsNone(a.load_levels(path))
   a.save_levels(path,target);self.assertEqual(a.load_levels(path),target)
   with self.assertRaises(RuntimeError):a.save_levels(path,{})
   self.assertEqual(a.load_levels(path),target)
 def test_late_alsa_devices_and_order(self):
  self.assertIn('DeviceAllow=char-alsa rw',a.AUDIO_DROP)
  self.assertIn('After=microduck-audio-prepare.service',a.AUDIO_DROP)
  self.assertIn('TimeoutStartSec=20',a.UNIT)
if __name__=='__main__':unittest.main()
