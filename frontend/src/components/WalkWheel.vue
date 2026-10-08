<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue';
import { wheelTwist } from '../wheelSpeed';
import type { Twist } from '../holdToWalk';
const props=defineProps<{disabled:boolean;speed:number}>();
const emit=defineEmits<{drive:[twist:Twist];release:[]}>();
const x=ref(0),y=ref(0);let pointer:number|null=null;let translating=false;
function stop(){pointer=null;translating=false;x.value=0;y.value=0;emit('release');}
function move(e:PointerEvent){
  if(pointer!==e.pointerId || props.disabled)return;
  const b=(e.currentTarget as HTMLElement).getBoundingClientRect();
  let dx=(e.clientX-b.left-b.width/2)/(b.width*.32),dy=(e.clientY-b.top-b.height/2)/(b.height*.32);
  const length=Math.hypot(dx,dy);if(length>1){dx/=length;dy/=length;}
  x.value=dx;y.value=dy;
  emit('drive',wheelTwist(dx,dy,props.speed));
}
function down(e:PointerEvent){if(props.disabled || pointer!==null || e.button!==0)return;pointer=e.pointerId;translating=true;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);move(e);}
function turn(e:PointerEvent,value:number){if(props.disabled || pointer!==null || e.button!==0)return;pointer=e.pointerId;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);emit('drive',[0,0,value]);}
function hidden(){if(document.hidden)stop();}
watch(()=>props.speed,()=>{if(translating && pointer!==null && !props.disabled)emit('drive',wheelTwist(x.value,y.value,props.speed));});
watch(()=>props.disabled,v=>{if(v)stop();});
onMounted(()=>{window.addEventListener('blur',stop);document.addEventListener('visibilitychange',hidden);});
onUnmounted(()=>{stop();window.removeEventListener('blur',stop);document.removeEventListener('visibilitychange',hidden);});
</script>
<template>
  <div class="wheel-control">
    <div class="wheel" :class="{disabled}" role="group" aria-label="方向轮盘：按住拖动，松开停止行走" @pointerdown.prevent="down" @pointermove="move" @pointerup="stop" @pointercancel="stop" @lostpointercapture="stop" @contextmenu.prevent>
      <span class="front">前进</span><span class="back">后退</span><span class="left">左移</span><span class="right">右移</span>
      <i :style="{transform:`translate(${x*50}px,${y*50}px)`}" />
    </div>
    <div class="wheel-help"><strong>按住拖动 · 松开平衡</strong><small>前后最高 {{ speed.toFixed(2) }} m/s<br>横移最高 {{ Math.min(speed,.1).toFixed(2) }} m/s</small>
      <div class="turns"><button :disabled="disabled" @pointerdown.prevent="turn($event,.5)" @pointerup="stop" @pointercancel="stop" @lostpointercapture="stop" @contextmenu.prevent>↶ 左转</button><button :disabled="disabled" @pointerdown.prevent="turn($event,-.5)" @pointerup="stop" @pointercancel="stop" @lostpointercapture="stop" @contextmenu.prevent>右转 ↷</button></div>
    </div>
  </div>
</template>
<style scoped>
.wheel-control{display:flex;align-items:center;justify-content:center;gap:18px;flex-wrap:wrap;padding:8px 0}.wheel{position:relative;width:160px;height:160px;border:1px solid #bdd0c3;border-radius:50%;background:radial-gradient(circle,#e2eee5 0 29%,#c1d5c6 30% 31%,#f4f9f5 32% 60%,#dcebe1 61%);touch-action:none;user-select:none;flex-shrink:0}.wheel.disabled{opacity:.45}.wheel span{position:absolute;font-size:12px;pointer-events:none;color:#426556}.front{top:12px;left:66px}.back{bottom:12px;left:66px}.left{left:9px;top:73px}.right{right:9px;top:73px}.wheel i{position:absolute;left:55px;top:55px;width:50px;height:50px;background:#4a765b;border-radius:50%;box-shadow:0 3px 8px #36584355;pointer-events:none;border:3px solid white;box-sizing:border-box}.wheel-help{display:flex;flex-direction:column;gap:8px;font-size:11px}.wheel-help small{font-size:10px}.turns{display:flex;gap:8px}.turns button{min-height:36px;min-width:60px;border:1px solid #bdd0c3;border-radius:9px;background:#f5faf6;color:#365843;touch-action:none;user-select:none}.turns button:disabled{opacity:.45}
</style>
