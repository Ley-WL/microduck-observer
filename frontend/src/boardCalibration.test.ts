import { afterEach,beforeEach,expect,it,vi } from "vitest";
import { createPinia,setActivePinia,disposePinia } from "pinia";
import { reactive,nextTick } from "vue";
import { useBoardCalibration } from "./boardCalibration";
import { calibrationKey } from "./savedCalibration";
import { bodyRelativeQuaternion } from "./calibration";
import { euler } from "./protocol";
const { state }=vi.hoisted(()=>({state:{current:null as any}}));
vi.mock("./store",()=>({useTelemetry:()=>state.current}));
let pinia:any, saved:Map<string,string>, server:any, fail=false;
const flush=async()=>{for(let i=0;i<20;i++)await Promise.resolve();await nextTick();};
it.each(["commanded","failed","enabled","configured","disabled"])("retains installation and position through repeated leveling after %s on a section-replacing server",async(control)=>{
  const installation={mountingQuaternion:[.5,.5,.5,.5],targetQuaternion:[0,-Math.SQRT1_2,0,Math.SQRT1_2],mountingSamples:{roll:[1,0,0]},mountingSamplesMethod:"gravity-v1"};
  server.imu={initialized:true,quaternion:[0,0,0,1],bootId:"boot",time:"saved",validForBoot:true,...installation};
  server.mounting={yaw:0,positionMm:[1,2,3]};
  const joints=JSON.parse(JSON.stringify(server.joints));
  Object.assign(state.current,{connection:"在线",paused:false,age:1,system:{data:{servoControl:{state:control}}},orientation:{valid:true,source:"hardware",bootId:"boot",data:{quaternion:[.5,.5,.5,.5]}}});
  const board=useBoardCalibration();await flush();expect(board.levelDisabledReason).toBe("");await board.levelOrientation();
  expect(board.error).toBe("");
  for(const [k,v] of Object.entries(installation))expect(server.imu[k]).toEqual(v);
  expect(server.mounting).toEqual({yaw:0,positionMm:[1,2,3]});expect(server.joints).toEqual(joints);
  const angles=euler(bodyRelativeQuaternion(server.imu.quaternion,state.current.orientation.data.quaternion,installation.mountingQuaternion,installation.targetQuaternion));
  expect(angles[0]).toBeCloseTo(0);expect(angles[1]).toBeCloseTo(0);
  const leveled=[...server.imu.quaternion];await board.levelOrientation();
  server.imu.quaternion.forEach((v:number,i:number)=>expect(v).toBeCloseTo(leveled[i]));
  await board.orientation(server.imu.quaternion,"boot","updated");
  for(const [k,v] of Object.entries(installation))expect(server.imu[k]).toEqual(v);
  expect(server.imu).not.toHaveProperty("validForBoot");
});
it("explains an active model or stale IMU and rejects leveling without a write",async()=>{
  server.imu={initialized:true,quaternion:[0,0,0,1],mountingQuaternion:[0,0,0,1],bootId:"boot",time:"saved"};
  Object.assign(state.current,{connection:"在线",paused:false,age:1,system:{data:{servoControl:{state:"policy"}}},orientation:{valid:true,source:"hardware",bootId:"boot",data:{quaternion:[0,0,0,1]}}});
  const board=useBoardCalibration();await flush();const revision=server.revision;
  expect(board.levelDisabledReason).toContain("停止模型");await board.levelOrientation();expect(server.revision).toBe(revision);
  state.current.system.data.servoControl.state="failed";state.current.age=1000;
  expect(board.levelDisabledReason).toContain("实时 IMU");await board.levelOrientation();expect(server.revision).toBe(revision);
});
beforeEach(()=>{
  vi.useFakeTimers();saved=new Map();fail=false;
  server={schema:1,deviceId:"duck",revision:0,bootId:"boot",joints:{initialized:false,references:{},directions:{}},imu:{initialized:false,quaternion:null,bootId:"",time:""}};
  vi.stubGlobal("localStorage",{getItem:(k:string)=>saved.get(k)||null});
  vi.stubGlobal("fetch",vi.fn(async(_url:string,options:any)=>{
    if(fail)throw new Error("offline");
    if(options?.method==="POST"){
      const body=JSON.parse(options.body);
      if(body.revision!==server.revision)return {ok:false,json:async()=>({detail:"conflict"})};
      for(const key of Object.keys(body.patch))server[key]={...body.patch[key],initialized:true};
      server.revision++;
    }
    const value=JSON.parse(JSON.stringify(server));return {ok:true,json:async()=>value};
  }));
  state.current=reactive({endpoint:"http://duck",joints:{source:"hardware"},orientation:null});
  pinia=createPinia();setActivePinia(pinia);
});
afterEach(()=>{disposePinia(pinia);vi.useRealTimers();vi.unstubAllGlobals();});
it("shares calibration with a new browser and loads edits from another client",async()=>{
  let board=useBoardCalibration();await flush();await board.joints(j=>{j.references[12]=4971;j.directions[12]=1;});
  disposePinia(pinia);pinia=createPinia();setActivePinia(pinia);board=useBoardCalibration();await flush();
  expect(board.data.joints.references[12]).toBe(4971);
  server.joints.references[12]=6000;server.revision++;await board.refresh();expect(board.data.joints.references[12]).toBe(6000);
});
it("migrates old browser only into an empty board",async()=>{
  saved.set(calibrationKey("joints",["http://duck","hardware"]),JSON.stringify({references:{12:4971},directions:{12:-1}}));
  const board=useBoardCalibration();await flush();expect(server.joints.references[12]).toBe(4971);
  await board.joints(j=>{j.references={};j.directions={};});await flush();await board.refresh();await flush();expect(server.joints.references).toEqual({});
});
it("keeps board authority on errors and concurrent changes",async()=>{
  const board=useBoardCalibration();await flush();server.revision++;server.joints.references[12]=5000;
  await board.joints(j=>{j.references[12]=6000;});expect(board.error).toContain("conflict");expect(board.data.joints.references[12]).toBe(5000);
  fail=true;await board.joints(j=>{j.references[12]=7000;});expect(board.data.joints.references[12]).toBe(5000);expect(board.error).toContain("offline");
});
it("does not overwrite board data with another browser legacy data",async()=>{
  server.joints={initialized:true,references:{12:8000},directions:{}};
  saved.set(calibrationKey("joints",["http://duck","hardware"]),JSON.stringify({references:{12:4971},directions:{}}));
  useBoardCalibration();await flush();expect(server.joints.references[12]).toBe(8000);expect(server.revision).toBe(0);
});

it("saves mounting independently and confirms an existing IMU reference without changing servo values",async()=>{
  server.joints={initialized:true,references:{12:8000},directions:{12:-1}};
  server.imu={initialized:true,quaternion:[0,0,0,1],bootId:"old",time:"12:00"};
  const board=useBoardCalibration();await flush();
  await board.mounting(90);
  expect(server.imu.bootId).toBe("old");
  expect(server.joints.references[12]).toBe(8000);
  await board.orientation([...board.data.imu.quaternion],"boot",board.data.imu.time);
  expect(server.imu.quaternion).toEqual([0,0,0,1]);
  expect(server.imu.bootId).toBe("boot");
  expect(server.mounting.yaw).toBe(90);
  expect(server.joints.references[12]).toBe(8000);
});

it("keeps supine target and measured axes when confirming a restored session",async()=>{
  const target=[0,-Math.SQRT1_2,0,Math.SQRT1_2];
  const axes=[0,0,Math.SQRT1_2,Math.SQRT1_2];
  server.imu={initialized:true,quaternion:[0,0,0,1],bootId:"old",time:"12:00",validForBoot:false,targetQuaternion:target,mountingQuaternion:axes,mountingSamples:{roll:[1,0,0]}};
  const board=useBoardCalibration();await flush();await board.confirmReference("boot");
  expect(server.imu).toEqual({...server.imu,bootId:"boot",targetQuaternion:target,mountingQuaternion:axes,mountingSamples:{roll:[1,0,0]}});
  expect(server.imu.time).toBe("12:00");
  expect(server.imu).not.toHaveProperty("validForBoot");
});

it("automatically reuses saved reference once a valid hardware session arrives",async()=>{
  server.imu={initialized:true,quaternion:[0,0,0,1],bootId:"old",time:"saved",mountingQuaternion:[0,0,0,1]};
  server.joints={initialized:true,references:{12:8000},directions:{}};
  const board=useBoardCalibration();await flush();
  expect(server.imu.bootId).toBe("old");
  state.current.orientation={valid:true,source:"hardware",bootId:"boot"};
  await flush();await flush();
  expect(server.imu.bootId).toBe("boot");
  expect(server.imu.quaternion).toEqual([0,0,0,1]);
  expect(server.joints.references[12]).toBe(8000);
  const revision=server.revision;
  await board.refresh();await flush();
  expect(server.revision).toBe(revision);
});
