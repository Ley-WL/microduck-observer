import copy
import importlib.util
from pathlib import Path

import numpy as np
import pytest

spec=importlib.util.spec_from_file_location('shadow_policy',Path(__file__).parents[1]/'scripts/shadow_policy.py')
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)


def fixture():
    names=[f'j{i}' for i in range(14)]
    meta={'joint_names':','.join(names),'default_joint_pos':','.join(['0']*14),'observation_names':'base_ang_vel,projected_gravity,joint_pos,joint_vel,actions,command,head_command,body_command'}
    geom={'bodies':[{'joint':{'id':i,'name':name,'range':[-1,1]}} for i,name in enumerate(names)]}
    sample={'valid':True,'source':'hardware','ageMs':0,'bootId':'a','sampleMonoMs':1000}
    snap={'calibration':{'deviceId':'duck','revision':1,'imu':{'bootId':'a','quaternion':[0,0,0,1]},'joints':{'references':{str(i):0 for i in range(14)},'directions':{}}},
          'imu.orientation':{**sample,'data':{'quaternion':[0,0,0,1]}},
          'imu.raw':{**sample,'data':{'gyro':[1,2,3]}},
          'joints':{**sample,'data':{'servos':[{'id':i,'online':True,'fault':0,'ageMs':0,'position':0} for i in range(14)]}}}
    return m.ObservationBuilder(meta,geom),snap


def test_unique_periodic_encoder_branch():
    assert m.joint_angle(0,4096,-1,[-1,1])==pytest.approx((0,-1))
    with pytest.raises(m.InvalidObservation):m.joint_angle(2048,0,-1,[-1,1])
    with pytest.raises(m.InvalidObservation):m.joint_angle(0,0,-1,[-7,7])


def test_contract_axis_velocity_and_history():
    builder,snap=fixture()
    with pytest.raises(m.InvalidObservation,match='warming'):builder.build(snap,np.zeros(14))
    snap['joints']['sampleMonoMs']+=20
    snap['joints']['data']['servos'][0]['position']=1
    obs,details=builder.build(snap,np.ones(14))
    assert obs.shape==(61,)
    np.testing.assert_allclose(obs[:6],[2,-1,3,0,0,-1])
    assert obs[20]==pytest.approx(-2*np.pi/4096/.02)
    np.testing.assert_equal(obs[34:48],np.ones(14))
    np.testing.assert_equal(obs[48:],np.zeros(13))
    assert details['raw_encoder'][0]==1
    q=[-np.sin(.1),0,0,np.cos(.1)]
    snap['imu.orientation']['data']['quaternion']=q
    obs,_=builder.build(snap,np.zeros(14))
    assert obs[3]>0 and obs[4]==pytest.approx(0)


@pytest.mark.parametrize('kind',['stale','missing','nan','boot','fault'])
def test_reject_bad_inputs(kind):
    builder,snap=fixture()
    if kind=='stale':snap['imu.raw']['ageMs']=150
    if kind=='missing':del snap['calibration']['joints']['references']['0']
    if kind=='nan':snap['imu.raw']['data']['gyro'][0]=float('nan')
    if kind=='boot':snap['imu.raw']['bootId']='b'
    if kind=='fault':snap['joints']['data']['servos'][0]['fault']=1
    with pytest.raises((m.InvalidObservation,KeyError)):builder.build(snap,np.zeros(14))


def test_board_mounting_used_and_old_reference_blocks_inference():
    builder,snap=fixture()
    snap['calibration']['mounting']={'yaw':90}
    with pytest.raises(m.InvalidObservation,match='warming'):builder.build(snap,np.zeros(14))
    obs,_=builder.build(snap,np.zeros(14))
    np.testing.assert_allclose(obs[:3],[-2,1,3])
    snap['calibration']['imu']['bootId']='old'
    with pytest.raises(m.InvalidObservation,match='another boot'):builder.build(snap,np.zeros(14))


def test_supine_reference_retains_world_gravity():
    builder,snap=fixture()
    snap['calibration']['mounting']={'yaw':0}
    snap['calibration']['imu']['targetQuaternion']=[0,-2**-.5,0,2**-.5]
    with pytest.raises(m.InvalidObservation,match='warming'):builder.build(snap,np.zeros(14))
    snap['joints']['sampleMonoMs']+=20
    obs,_=builder.build(snap,np.zeros(14))
    np.testing.assert_allclose(obs[3:6],[-1,0,0],atol=1e-6)
