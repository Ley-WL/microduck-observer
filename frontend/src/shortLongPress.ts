export function shortLongPress(short:()=>void, long:()=>void, delay=700) {
  let timer:ReturnType<typeof setTimeout>|undefined, down=false, fired=false;
  function cancel(){down=false;if(timer!==undefined)clearTimeout(timer);timer=undefined;}
  return {
    start(){if(down)return;down=true;fired=false;timer=setTimeout(()=>{timer=undefined;if(down){fired=true;long();}},delay);},
    release(){const tap=down && !fired;cancel();if(tap)short();},
    cancel,
  };
}
