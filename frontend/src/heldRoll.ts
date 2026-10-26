// Refresh upstream's 150 ms chain window while the button is held.
export function heldRoll(fire:()=>void, enabled:()=>boolean) {
  let timer:ReturnType<typeof setInterval>|undefined;
  function cancel(){if(timer!==undefined)clearInterval(timer);timer=undefined;}
  function start(){
    if(timer!==undefined || !enabled())return;
    timer=setInterval(()=>{if(enabled())fire();else cancel();},60);
    fire();
  }
  return {start,cancel};
}
