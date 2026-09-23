import { afterEach,beforeEach,expect,it,vi } from "vitest";
import { createPinia,setActivePinia,disposePinia } from "pinia";
import { reactive,nextTick } from "vue";
import { useBoardCalibration } from "./boardCalibration";
import { calibrationKey } from "./savedCalibration";
const { state }=vi.hoisted(()=>({state:{current:null as any}}));
vi.mock("./store",()=>({useTelemetry:()=>state.current}));
let pinia:any, saved:Map<string,string>, server:any, fail=false;
const flush=async()=>{for(let i=0;i<20;i++)await Promise.resolve();await nextTick();};
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
