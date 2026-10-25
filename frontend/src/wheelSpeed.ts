import type { Twist } from './holdToWalk';
/** A single stick commands forward/backward and yaw, like DuckLink. */
export function wheelTwist(x:number,y:number,speed:number):Twist {
  if (![x,y,speed].every(Number.isFinite)) return [0,0,0];
  const length=Math.hypot(x,y);if(length<.15)return [0,0,0];
  const scale=Math.max(1,length),forward=Math.min(.4,Math.max(.02,speed));
  return [-y/scale*forward,0,-x/scale*1];
}
export function gamepadSpeed(base:number,boost:boolean):number {
  if(!Number.isFinite(base))return .2;
  return Math.min(.4,Math.max(.02,base)*(boost?2:1));
}

/** Swizzle uses throttle and heading error, independently of the walking slider. */
export function rollerTwist(x:number,y:number):Twist {
  if (![x,y].every(Number.isFinite) || Math.hypot(x,y)<.15) return [0,0,0];
  const scale=Math.max(1,Math.hypot(x,y));
  return [-y/scale*.4,0,-x/scale*.25];
}
