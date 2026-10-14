import type { Twist } from './holdToWalk';
export type PadInput = { twist: Twist; headPitch: number; headYaw: number };
/** Independent pointers share one sequence and renew only while either stick is held. */
export function gamepadInput(send:(input:PadInput,sequence:number)=>Promise<void>, next:()=>number, onError:(e:unknown)=>void) {
  let twist:Twist=[0,0,0], head=[0,0], bodyHeld=false, headHeld=false;
  let timer:ReturnType<typeof setInterval>|undefined, generation=0;
  const snapshot=():PadInput=>({twist:[...twist],headPitch:head[0]!,headYaw:head[1]!});
  function dispatch(){const id=generation;void send(snapshot(),next()).catch(e=>{if(id!==generation)return;release();onError(e);});}
  function update(){generation++;dispatch();if(bodyHeld||headHeld){if(!timer)timer=setInterval(dispatch,80);}else{if(timer)clearInterval(timer);timer=undefined;}}
  function release(){const held=bodyHeld||headHeld;bodyHeld=headHeld=false;twist=[0,0,0];head=[0,0];generation++;if(timer)clearInterval(timer);timer=undefined;if(held)void send(snapshot(),next()).catch(()=>{});}
  return {body(value:Twist|null){if(value===null&&!bodyHeld)return;bodyHeld=value!==null;twist=value??[0,0,0];update();},head(value:[number,number]|null){if(value===null&&!headHeld)return;headHeld=value!==null;head=value??[0,0];update();},release,nextSequence:next};
}
