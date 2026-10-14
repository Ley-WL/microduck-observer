<script setup lang="ts">
import PadStick from './PadStick.vue';
import {wheelTwist} from '../wheelSpeed';
import type {Twist} from '../holdToWalk';
const props=defineProps<{drive:boolean;head:boolean;speed:number;reset:number;allowed:Record<string,boolean>;active:boolean}>();
const emit=defineEmits<{action:[key:string];body:[value:Twist|null];head:[value:[number,number]|null]}>();
function body(x:number,y:number){emit('body',wheelTwist(x,y,props.speed));}
</script>
<template>
<div class="pad-wrap">
  <div class="gamepad" aria-label="T4 Nova Lite 风格虚拟手柄">
    <svg class="pad-silhouette" viewBox="0 0 640 350" aria-hidden="true"><defs><linearGradient id="pad-shell" x2="0.3" y2="1"><stop stop-color="#515060"/><stop offset="1" stop-color="#262631"/></linearGradient></defs><path d="M144 35 Q192 16 253 41 Q320 54 387 41 Q448 16 496 35 Q548 49 570 115 L616 281 Q625 334 588 337 Q563 340 541 310 L474 290 Q320 260 166 290 L99 310 Q77 340 52 337 Q15 334 24 281 L70 115 Q92 49 144 35Z" fill="url(#pad-shell)" stroke="#656373" stroke-width="2"/><path d="M166 290 Q320 260 474 290" fill="none" stroke="#171720" stroke-width="3"/></svg>
    <button class="shoulder left" :disabled="!allowed.pick" @click="emit('action','pick')"><b>LB</b> 拾取</button>
    <button class="shoulder right" :disabled="!allowed.quack" @click="emit('action','quack')">嘎一下 <b>RB</b></button>
    <div class="stick-left"><PadStick label="L · 行走 / 转向" :disabled="!drive" :reset="reset" @move="body" @release="emit('body',null)" /></div>
    <div class="stick-right"><PadStick label="R · 头部" :disabled="!head" :reset="reset" @move="(x,y)=>emit('head',[-y,-x])" @release="emit('head',null)" /></div>
    <div class="pad-brand"><span :class="{online:active}"/><strong>MICRODUCK</strong><small>VIRTUAL CONTROLLER</small></div>
    <button class="menu-button back" :disabled="!allowed.stop" aria-label="Back 停止保持" @click="emit('action','stop')">▱<small>停止</small></button>
    <button class="menu-button start" :disabled="!allowed.standup" aria-label="Start 站起" @click="emit('action','standup')">▷<small>开始</small></button>
    <div class="face-buttons">
      <button class="face y" :disabled="!allowed.getup" aria-label="Y 倒地起身" @click="emit('action','getup')"><b>Y</b><small>起身</small></button>
      <button class="face x" :disabled="!allowed.sit" aria-label="X 坐下" @click="emit('action','sit')"><b>X</b><small>坐下</small></button>
      <button class="face b" :disabled="!allowed.stop" aria-label="B 停止保持" @click="emit('action','stop')"><b>B</b><small>停止</small></button>
      <button class="face a" :disabled="!allowed.standup" aria-label="A 站起" @click="emit('action','standup')"><b>A</b><small>站起</small></button>
    </div>
    <div class="dpad" aria-label="策略动作">
      <button :disabled="!allowed.pick" @click="emit('action','pick')" aria-label="十字键上 拾取">↑<small>拾取</small></button>
      <button disabled title="官方左脚踢球策略待接入" aria-label="十字键左 左脚踢球，待接入"><small>左脚<br>踢球</small></button>
      <span/>
      <button disabled title="官方右脚踢球策略待接入" aria-label="十字键右 右脚踢球，待接入"><small>右脚<br>踢球</small></button>
      <button disabled title="自主行动：预留入口，后续接入自主策略" aria-label="十字键下 自主行动，待接入"><small>自主<br>行动</small></button>
    </div>
    <span class="grip left-grip">按住拖动</span><span class="grip right-grip">松开回中</span>
  </div>
  <div class="pad-footer"><span><i/> 左右踢球 / 自主行动待接入</span><button class="estop" :disabled="!allowed.disable" @click="emit('action','disable')">■ 急停卸力</button></div>
</div>
</template>
<style scoped>
.pad-wrap{width:100%;max-width:760px;margin:auto}.gamepad{position:relative;width:100%;aspect-ratio:640/350;container-type:inline-size;filter:drop-shadow(0 12px 10px #19192324);margin:18px auto 0}.pad-silhouette{position:absolute;width:100%;height:100%;inset:0}.gamepad button{font:inherit;cursor:pointer;color:#eeedf4;border:1px solid #17171e;box-shadow:inset 0 1px 1px #ffffff22,0 3px 3px #0006;background:linear-gradient(#44434e,#24242d);touch-action:manipulation;user-select:none}.gamepad button:disabled{opacity:.38;cursor:not-allowed}.gamepad button:not(:disabled):active{transform:translateY(2px);box-shadow:inset 0 2px 5px #0008}.gamepad button:focus-visible{outline:3px solid #ec985e;outline-offset:3px}.shoulder{position:absolute;top:1%;width:22%;height:8%;border-radius:50% 50% 6px 6px;font-size:clamp(10px,2cqw,14px)!important}.shoulder.left{left:13%}.shoulder.right{right:13%}.shoulder b{color:#efaa71;margin:0 8px}.stick-left{position:absolute;width:18%;left:11%;top:18%}.stick-right{position:absolute;width:18%;left:58%;top:40%}.pad-brand{position:absolute;left:39%;top:29%;width:22%;display:flex;flex-direction:column;align-items:center;gap:5px;color:#aaa7b8}.pad-brand>span{height:5px;width:22px;border-radius:4px;background:#7c7a87}.pad-brand>span.online{background:#dba570;box-shadow:0 0 8px #eaa36288}.pad-brand strong{font-size:clamp(8px,1.8cqw,13px);letter-spacing:2px}.pad-brand small{font-size:clamp(6px,1.1cqw,9px);letter-spacing:1px}.menu-button{position:absolute;top:21%;width:5%;height:7%;border-radius:50%;font-size:12px!important}.menu-button small{position:absolute;top:130%;left:-25%;width:150%;font-size:9px;color:#c5c2ce;white-space:nowrap}.back{left:35%}.start{right:35%}.face-buttons{position:absolute;right:10%;top:15%;width:20%;height:37%}.face{position:absolute;width:34%;height:34%;border-radius:50%;display:flex;flex-direction:column;align-items:center;justify-content:center;padding:0;gap:0}.face b{font-size:clamp(10px,2.5cqw,18px)}.face small{font-size:clamp(7px,1.5cqw,10px);color:#ccc8d5}.face.y{left:33%;top:0;color:#eacb75}.face.x{left:0;top:33%;color:#91bfea}.face.b{right:0;top:33%;color:#f28d8a}.face.a{left:33%;bottom:0;color:#a9d7ac}.dpad{position:absolute;left:30%;top:49%;width:18%;height:31%;display:grid;grid-template:repeat(3,1fr)/repeat(3,1fr);filter:drop-shadow(0 3px 2px #0008)}.dpad button{padding:0;min-width:0;min-height:0;line-height:1.05;border:0;border-radius:4px;display:flex;align-items:center;justify-content:center;flex-direction:column;font-size:clamp(10px,2.2cqw,16px)}.dpad button small{font-size:clamp(7px,1.4cqw,10px);white-space:nowrap}.dpad button:first-child{grid-column:2}.dpad button:nth-child(2){grid-column:1;grid-row:2}.dpad span{grid-column:2;grid-row:2;background:#30303a}.dpad button:nth-child(4){grid-column:3;grid-row:2}.dpad button:last-child{grid-column:2;grid-row:3}.grip{position:absolute;top:78%;font-size:clamp(8px,1.7cqw,12px);color:#a6a3b3;letter-spacing:1px}.left-grip{left:10%;transform:rotate(-35deg)}.right-grip{right:10%;transform:rotate(35deg)}.pad-footer{display:flex;align-items:center;justify-content:space-between;gap:8px;padding:5px 3%;font-size:11px;color:#77867d}.pad-footer i{display:inline-block;width:6px;height:6px;border-radius:50%;background:#b37850;margin-right:6px}.estop{border:1px solid #b95748;color:#a33129;border-radius:8px;background:#fff0eb;padding:10px 14px;font-weight:700;cursor:pointer}.estop:disabled{opacity:.4;cursor:not-allowed}
@media(max-width:500px){.pad-footer{font-size:10px}.stick-left :deep(.stick-label),.stick-right :deep(.stick-label){font-size:9px}.menu-button small{font-size:8px}.estop{padding:9px}.pad-brand strong{letter-spacing:1px}}
@container(max-width:440px){.pad-brand small{display:none}.pad-brand strong{font-size:8px;letter-spacing:.5px}.face-buttons{width:24%;right:7%;height:40%}}
</style>
