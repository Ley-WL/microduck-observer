import type {Sample} from './protocol';
export const buttonNames = ['a','b','x','y','lb','rb','back','start','up','left','right','down'] as const;
export type GamepadFeedback = {connected:boolean;device:string;buttons:Record<string,boolean>;axes:number[]};
export function validGamepad(data:any):data is GamepadFeedback {
  return data && typeof data.connected==='boolean' && typeof data.device==='string' &&
    data.buttons && buttonNames.every(key=>typeof data.buttons[key]==='boolean') &&
    Array.isArray(data.axes) && data.axes.length===4 && data.axes.every((v:unknown)=>typeof v==='number' && Number.isFinite(v) && Math.abs(v)<=1);
}
export function freshGamepad(sample:Sample|null,age:number,online:boolean):GamepadFeedback|null {
  return online && sample?.source==='hardware' && sample.valid && age>=0 && age<500 &&
    validGamepad(sample.data) && sample.data.connected ? sample.data : null;
}
