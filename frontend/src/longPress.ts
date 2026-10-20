export function longPress(fire:()=>void, delay=1500){
  let timer:ReturnType<typeof setTimeout>|undefined;
  let down=false;
  return {
    start(){if(down)return;down=true;timer=setTimeout(()=>{timer=undefined;if(down)fire();},delay);},
    cancel(){down=false;if(timer!==undefined)clearTimeout(timer);timer=undefined;},
  };
}
