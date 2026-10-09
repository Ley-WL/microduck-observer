import type { Twist } from './holdToWalk';
/** A single stick commands forward/backward and yaw, like DuckLink. */
export function wheelTwist(x:number,y:number,speed:number):Twist {
  if (![x,y,speed].every(Number.isFinite)) return [0,0,0];
  const length=Math.hypot(x,y);if(length<.15)return [0,0,0];
  const scale=Math.max(1,length),forward=Math.min(.2,Math.max(.02,speed));
  return [-y/scale*forward,0,-x/scale*.5];
}
