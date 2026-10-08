import { expect,it } from 'vitest';
import { driveLease } from './driveLease';
it('restores our own lease and monotonic sequence across remounts and isolates boards',()=>{
  const values=new Map<string,string>(),storage={getItem:(key:string)=>values.get(key)||null,setItem:(key:string,value:string)=>{values.set(key,value);}};
  const first=driveLease(storage,'http://board/');first.remember('ours');expect(first.nextSequence()).toBe(1);
  const refreshed=driveLease(storage,'http://board');expect(refreshed.session).toBe('ours');expect(refreshed.nextSequence()).toBe(2);
  expect(driveLease(storage,'http://other').session).toBe('');
  refreshed.remember('new');expect(refreshed.nextSequence()).toBe(1);
});
it('ignores malformed storage',()=>{
  const lease=driveLease({getItem:()=>'{bad',setItem:()=>{}},'http://board');expect(lease.session).toBe('');expect(lease.nextSequence()).toBe(1);
});
