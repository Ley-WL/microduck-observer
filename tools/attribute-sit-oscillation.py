"""Exact adjacent-input Shapley attribution and frozen-sensor diagnostics.

These isolate actor responses, not causal changes to a physical closed loop.
Always load the model whose SHA is in the captured trace, never current blindly.
"""
import argparse,hashlib,itertools,json,math
from pathlib import Path
import numpy as np
import onnx
from onnx import numpy_helper
import onnxruntime as ort
import torch
from torch import nn

BLOCKS={'gyro':slice(0,3),'gravity':slice(3,6),'position':slice(6,20),
        'velocity':slice(20,34),'history':slice(34,48)}


def attribute(session, old, new):
    values={}
    keys=list(BLOCKS)
    for mask in range(32):
        obs=old.copy()
        for i,key in enumerate(keys):
            if mask & (1<<i):obs[BLOCKS[key]]=new[BLOCKS[key]]
        values[mask]=session.run(None,{session.get_inputs()[0].name:obs[None]})[0][0]
    contributions={}
    for i,key in enumerate(keys):
        total=np.zeros(14)
        for mask in range(32):
            if mask & (1<<i):continue
            size=mask.bit_count()
            weight=math.factorial(size)*math.factorial(4-size)/math.factorial(5)
            total+=weight*(values[mask|(1<<i)]-values[mask])
        contributions[key]=total
    return contributions,values[31]-values[0]


def actor_torch(path):
    tensors={t.name:numpy_helper.to_array(t).copy() for t in onnx.load(path).graph.initializer}
    mlp=nn.Sequential(nn.Linear(61,512),nn.ELU(),nn.Linear(512,256),nn.ELU(),nn.Linear(256,128),nn.ELU(),nn.Linear(128,14))
    mlp.load_state_dict({k.removeprefix('mlp.'):torch.tensor(v) for k,v in tensors.items() if k.startswith('mlp.')})
    mean=torch.tensor(tensors['obs_normalizer._mean'][0]);div=torch.tensor(tensors['onnx::Div_24'][0])
    return lambda x:mlp((x-mean)/div)


def main(a):
    torch.set_num_threads(2)
    trace=json.loads(a.trace.read_text(encoding='utf-8'))
    rows=[r for r in trace['frames'] if r['type']=='inference' and r['kind']=='sitstand_sit']
    sha=hashlib.sha256(a.model.read_bytes()).hexdigest()
    assert all(r['policySha256']==sha for r in rows), 'Trace/model mismatch'
    first=rows[0]['mono'];early=[r for r in rows if r['mono']<first+2]
    opts=ort.SessionOptions();opts.intra_op_num_threads=1
    session=ort.InferenceSession(str(a.model),sess_options=opts,providers=['CPUExecutionProvider'])
    events=[];all_contributions=[];errors=[]
    for old,new in zip(early,early[1:]):
        if new['mono']-old['mono']>.04:continue
        x=np.array(old['observation'],dtype=np.float32);y=np.array(new['observation'],dtype=np.float32)
        np.testing.assert_array_equal(x[48:],y[48:])
        c,delta=attribute(session,x,y)
        error=float(np.max(np.abs(sum(c.values())-delta)));errors.append(error)
        assert error<1e-5, 'Adjacent-block attribution did not reconstruct actor change'
        all_contributions.append(np.degrees(np.array(list(c.values())))[:,[3,12]])
        events.append({'time':new['mono']-first,'rawKneeDeltaDeg':np.degrees(delta[[3,12]]).tolist(),
                       'contributionDeg':{k:np.degrees(v[[3,12]]).tolist() for k,v in c.items()},
                       'positionKneesDeg':np.degrees(y[6:20][[3,12]]+np.array(new['homeRadians'])[[3,12]]).tolist(),
                       'velocityKneesRadS':y[20:34][[3,12]].tolist(),
                       'gyroRadS':y[:3].tolist()})
    actor=actor_torch(a.model)
    jacobians=[];frozen=[]
    for index in (0,5,10,20,30,50,70,90,len(rows)-1):
        row=rows[index];obs=np.array(row['observation'],dtype=np.float32)
        np.testing.assert_allclose(actor(torch.tensor(obs)).detach().numpy(),session.run(None,{session.get_inputs()[0].name:obs[None]})[0][0],atol=2e-5,rtol=2e-5)
        jac=torch.autograd.functional.jacobian(actor,torch.tensor(obs)).detach().numpy()
        history=jac[:,34:48];eig=np.linalg.eigvals(history)
        jacobians.append({'time':row['mono']-first,'historySpectralRadius':float(np.abs(eig).max()),
                          'kneeGyroDerivativesRadPerRadS':jac[[3,12],:3].tolist(),
                          'leftKneePositionSelfDerivative':float(jac[3,6+3]),
                          'rightKneePositionSelfDerivative':float(jac[12,6+12]),
                          'leftKneeVelocitySelfDerivative':float(jac[3,20+3]),
                          'rightKneeVelocitySelfDerivative':float(jac[12,20+12])})
        output=[]
        for _ in range(150):
            action=session.run(None,{session.get_inputs()[0].name:obs[None]})[0][0]
            output.append(action.copy());obs[34:48]=action
        output=np.array(output)
        frozen.append({'time':row['mono']-first,'finite':bool(np.isfinite(output).all()),
                       'last1sKneeTravelDeg':np.abs(np.diff(np.degrees(output[-50:, [3,12]]),axis=0)).sum(axis=0).tolist(),
                       'maxAbsActionRad':float(np.abs(output).max())})
    contributions=np.array(all_contributions)
    ranking=np.abs(contributions).sum(axis=0)
    def input_stats(observations):
        x=np.array(observations)
        return {'gyroNormP95RadS':float(np.percentile(np.linalg.norm(x[:,:3],axis=1),95)),
                'gyroNormMaxRadS':float(np.linalg.norm(x[:,:3],axis=1).max()),
                'kneeVelocityAbsP95RadS':np.percentile(np.abs(x[:,20:34][:,[3,12]]),95,axis=0).tolist(),
                'gyroFrameChangeP95RadS':float(np.percentile(np.linalg.norm(np.diff(x[:,:3],axis=0),axis=1),95))}
    observations=np.array([r['observation'] for r in early]);times=np.array([r['mono'] for r in early])
    dg=np.diff(observations[:,3:6],axis=0)/np.diff(times)[:,None]
    predicted=-np.cross((observations[1:,:3]+observations[:-1,:3])*.5,
                         (observations[1:,3:6]+observations[:-1,3:6])*.5)
    consistency={'scope':'Filtered gyro vs sampled gravity derivative; IMU filtering/timestamp offsets can affect correlation.',
                 'derivativeRmsRadS':float(np.sqrt(np.mean(dg**2))),
                 'predictedDerivativeRmsRadS':float(np.sqrt(np.mean(predicted**2))),
                 'flattenedCorrelation':float(np.corrcoef(dg.flatten(),predicted.flatten())[0,1]),
                 'oppositeSignCorrelation':float(np.corrcoef(dg.flatten(),-predicted.flatten())[0,1])}
    simulation_stats=[]
    if a.simulation:
        for path in sorted(a.simulation.glob('*.policy.json')):
            sim=json.loads(path.read_text(encoding='utf-8'))
            starts=[i for i,r in enumerate(sim) if i>0 and r['sit'] and not sim[i-1]['sit']]
            for start in starts:
                window=[r for r in sim[start:] if sim[start]['time']<=r['time']<sim[start]['time']+2]
                simulation_stats.append({'file':path.name,'time':sim[start]['time'],**input_stats([r['observation'] for r in window])})
    report={'sourceSha256':sha,'frames':len(early),'pairs':len(events),
            'exactAttributionMaxResidualRad':max(errors),
            'realFirst2sInputStats':input_stats(observations),
            'gyroGravityConsistency':consistency,'simulationSitInputStats':simulation_stats,
            'absoluteContributionTravelDeg':{k:ranking[i].tolist() for i,k in enumerate(BLOCKS)},
            'topEvents':sorted(events,key=lambda r:max(abs(v) for v in r['rawKneeDeltaDeg']),reverse=True)[:12],
            'jacobians':jacobians,'frozenSensorHistoryRollouts':frozen,
            'limits':'Attribution averages all input-block replacement orders. Correlated blocks are not independent physical causes; absolute contributions can exceed total travel through cancellation. Frozen sensors test only inner action-history loop. Position/velocity feedback and physical actuator gain/delay need system identification to prove outer-loop instability.'}
    a.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({k:v for k,v in report.items() if k not in ('topEvents','limits')}))


if __name__=='__main__':
    p=argparse.ArgumentParser()
    for name in ('trace','model','output'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--simulation',type=Path)
    main(p.parse_args())
