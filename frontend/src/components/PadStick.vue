<script setup lang="ts">
import {ref,watch,onMounted,onUnmounted} from 'vue';
const props=defineProps<{disabled:boolean;label:string;reset:number;feedback?:number[]|null}>();
const emit=defineEmits<{move:[x:number,y:number];release:[]}>();
const x=ref(0),y=ref(0);let pointer:number|null=null;let element:HTMLElement|null=null;
function stop(){const id=pointer;pointer=null;x.value=y.value=0;if(id!==null){if(element?.hasPointerCapture(id))element.releasePointerCapture(id);emit('release');}}
function move(e:PointerEvent){if(pointer!==e.pointerId||props.disabled)return;const r=(e.currentTarget as HTMLElement).getBoundingClientRect();let dx=(e.clientX-r.left-r.width/2)/(r.width*.32),dy=(e.clientY-r.top-r.height/2)/(r.height*.32);const n=Math.max(1,Math.hypot(dx,dy));dx/=n;dy/=n;x.value=dx;y.value=dy;emit('move',Math.abs(dx)<.15?0:dx,Math.abs(dy)<.15?0:dy);}
function down(e:PointerEvent){if(props.disabled||pointer!==null||e.button!==0)return;pointer=e.pointerId;element=e.currentTarget as HTMLElement;element.setPointerCapture(pointer);move(e);}
function end(e:PointerEvent){if(pointer===e.pointerId)stop();}
function hidden(){if(document.hidden)stop();}
watch(()=>[props.disabled,props.reset],()=>stop());
onMounted(()=>{window.addEventListener('blur',stop);document.addEventListener('visibilitychange',hidden);});
onUnmounted(()=>{stop();window.removeEventListener('blur',stop);document.removeEventListener('visibilitychange',hidden);});
</script>
<template><div class="stick-unit"><div class="stick" :class="{disabled,held:pointer!==null,physical:!!feedback && feedback.some(v=>v!==0)}" role="group" :aria-label="label+'：按住拖动'" :aria-disabled="disabled" @pointerdown.prevent="down" @pointermove="move" @pointerup="end" @pointercancel="end" @lostpointercapture="end" @contextmenu.prevent><span class="axis horizontal"/><span class="axis vertical"/><i :style="{transform:`translate(${(feedback?.[0]??x)*24}%,${(feedback?.[1]??y)*24}%)`}"><span/></i></div><span class="stick-label">{{label}}</span></div></template>
<style scoped>
.stick.physical{opacity:1!important;outline:2px solid #ffb86e;box-shadow:0 0 12px #ff9b52}

.stick-unit{width:100%;text-align:center}.stick{aspect-ratio:1;position:relative;border-radius:50%;background:radial-gradient(circle,#15161e 0 55%,#454553 57%,#24252f 65%);box-shadow:inset 0 3px 6px #0009,0 2px 1px #ffffff1a;touch-action:none;user-select:none;cursor:grab}.stick.disabled{opacity:.55;cursor:not-allowed}.stick i{position:absolute;inset:18%;border-radius:50%;background:linear-gradient(145deg,#ed9952,#b44b19);box-shadow:0 4px 7px #0009;padding:4px;box-sizing:border-box;pointer-events:none}.stick i span{display:block;width:100%;height:100%;border-radius:50%;background:radial-gradient(circle at 40% 30%,#454550,#25262e 75%);box-shadow:inset 0 2px 2px #ffffff20;border:1px solid #111}.stick-label{display:block;color:#dddce5;font-size:10px;margin-top:6px;letter-spacing:0;white-space:nowrap}.axis{position:absolute;background:#ffffff12;pointer-events:none}.horizontal{left:9%;right:9%;height:1px;top:50%}.vertical{top:9%;bottom:9%;width:1px;left:50%}
</style>
