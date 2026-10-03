import { Quaternion, Euler } from 'three';
import { mkdir, writeFile } from 'node:fs/promises';
const base='http://192.168.31.186:8877/api/v1/';
const dir=new URL('../docs/实测记录/附件/IMU/2026-10-03/',import.meta.url);
await mkdir(dir,{recursive:true});
async function get(path){const r=await fetch(base+path);if(!r.ok)throw Error(r.status);return r.json();}
const cal=await get('calibration');
const q=a=>new Quaternion().fromArray(a).normalize();
const m=q(cal.imu.mountingQuaternion), t=q(cal.imu.targetQuaternion??[0,0,0,1]);
const body=(ref,cur)=>t.clone().multiply(m.clone().invert()).multiply(ref.clone().invert()).multiply(cur).multiply(m);
const angles=b=>new Euler().setFromQuaternion(b,'ZYX');
const samples=[]; let sum=[0,0,0,0], anchor;
for(let n=0;n<25;n++){
  const s=await get('snapshot'), o=s['imu.orientation'];
  if(!o?.valid||o.ageMs>500||o.bootId!==cal.bootId)throw Error('IMU数据或会话无效');
  const cur=q(o.data.quaternion), e=angles(body(q(cal.imu.quaternion),cur));
  const desired=new Quaternion();
  const ref=cur.clone().multiply(m).multiply(desired.clone().invert()).multiply(t).multiply(m.clone().invert()).normalize();
  const a=ref.toArray();anchor??=a; const sign=a.reduce((v,x,i)=>v+x*anchor[i],0)<0?-1:1;
  sum=sum.map((v,i)=>v+sign*a[i]);samples.push({snapshot:s,angles:[e.x,e.y,e.z].map(v=>v*180/Math.PI)});
  await new Promise(r=>setTimeout(r,60));
}
const ref=q(sum), last=samples.at(-1), predicted=angles(body(ref,q(last.snapshot['imu.orientation'].data.quaternion)));
await writeFile(new URL('standing-reference-before.json',dir),JSON.stringify({cal,samples,predicted:[predicted.x,predicted.y,predicted.z].map(v=>v*180/Math.PI)}));
const imu={...cal.imu,quaternion:ref.toArray(),bootId:cal.bootId,time:new Date().toISOString()}; delete imu.validForBoot;
const response=await fetch(base+'calibration',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({revision:cal.revision,patch:{imu}})});
const saved=await response.json();if(!response.ok)throw Error(JSON.stringify(saved));
const after=await get('calibration');
if(JSON.stringify(after.joints)!==JSON.stringify(cal.joints)||JSON.stringify(after.imu.mountingQuaternion)!==JSON.stringify(cal.imu.mountingQuaternion))throw Error('不相关标定变化');
const results=[];
for(let n=0;n<10;n++){const s=await get('snapshot');const e=angles(body(q(after.imu.quaternion),q(s['imu.orientation'].data.quaternion)));results.push([e.x,e.y,e.z].map(v=>v*180/Math.PI));await new Promise(r=>setTimeout(r,80));}
await writeFile(new URL('standing-reference-after.json',dir),JSON.stringify({saved,after,results}));
console.log(JSON.stringify({revision:after.revision,before:last.angles,after:results,jointsPreserved:true,mountingPreserved:true}));
