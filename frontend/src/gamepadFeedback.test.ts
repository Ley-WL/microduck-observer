import {describe,it,expect} from 'vitest';
import {buttonNames,freshGamepad,validGamepad} from './gamepadFeedback';
import type {Sample} from './protocol';
const data=()=>({connected:true,device:'test-pad',buttons:Object.fromEntries(buttonNames.map(k=>[k,k==='a'||k==='up'])),axes:[0.5,-1,0,1]});
const sample=():Sample=>({type:'sample',protocolVersion:1,bootId:'test',topic:'gamepad',seq:1,sampleMonoMs:0,ageMs:0,source:'hardware',valid:true,data:data()});
describe('physical gamepad feedback',()=>{
 it('shows raw inputs including reserved keys without needing a motion session',()=>{
  expect(freshGamepad(sample(),40,true)?.buttons.up).toBe(true);
  expect(freshGamepad(sample(),40,true)?.axes).toEqual([0.5,-1,0,1]);
 });
 it('clears on stale, disconnected, invalid and nonhardware samples',()=>{
  expect(freshGamepad(sample(),500,true)).toBeNull();
  expect(freshGamepad(sample(),0,false)).toBeNull();
  for(const update of [{valid:false},{source:'simulation'},{data:{...data(),connected:false}}]) {
   expect(freshGamepad({...sample(),...update} as Sample,0,true)).toBeNull();
  }
 });
 it('rejects malformed axes and button payloads',()=>{
  expect(validGamepad({...data(),axes:[NaN,0,0,0]})).toBe(false);
  expect(validGamepad({...data(),axes:[2,0,0,0]})).toBe(false);
  expect(validGamepad({...data(),buttons:{a:true}})).toBe(false);
 });
});
