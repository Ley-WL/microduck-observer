<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { holdToWalk, type Twist } from '../holdToWalk';
import { useTelemetry } from '../store';
import { useBoardCalibration } from '../boardCalibration';
const state=useTelemetry(),board=useBoardCalibration();
const modelAvailable=ref({stand:false,walk:false,xgoduck:false}),kind=ref<'stand'|'walk'|'xgoduck'>('xgoduck'),speed=ref(0.2);
const session=ref(''),pressed=ref('');
const directions: {name:string;twist:Twist}[]=[{name:'前进',twist:[.2,0,0]},{name:'后退',twist:[-.2,0,0]},{name:'左移',twist:[0,.1,0]},{name:'右移',twist:[0,-.1,0]},{name:'左转',twist:[0,0,.5]},{name:'右转',twist:[0,0,-.5]}];
const available=computed(()=>modelAvailable.value[kind.value]);
const poseName=computed(()=>kind.value==='xgoduck'?'预备姿势':kind.value==='walk'?'模型行走姿势':'模型站立姿势');
const pending=ref(false),message=ref(''),failed=ref(false);
const control=computed(()=>state.system?.data.servoControl);
const running=computed(()=>control.value?.action==='policy' && ['preflight','moving','policy'].includes(control.value?.state));
const busy=computed(()=>pending.value || ['preflight','moving','policy','disabling'].includes(control.value?.state));
const live=computed(()=>state.connection==='在线' && state.joints?.source==='hardware' && state.jointsAge<500);
const canDrive=computed(()=>kind.value==='xgoduck' && live.value && control.value?.state==='policy' && control.value?.kind==='xgoduck' && !control.value?.feedbackHolding && !!session.value && !pending.value);
const hold=holdToWalk(async(twist,sequence)=>{
  const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/policy/command',{
    method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({session:session.value,sequence,twist}),keepalive:true,signal:AbortSignal.timeout(1000)});
  if(!response.ok){const result=await response.json();throw new Error(result.detail || '行走指令未接受');}
});
function release(){pressed.value='';hold.release();}
function press(event:PointerEvent,name:string,twist:Twist){
  if(!canDrive.value || pressed.value || event.button!==0)return;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  pressed.value=name;hold.press(twist);
}
function hidden(){if(document.hidden)release();}
watch(canDrive,value=>{if(!value)release();});
onMounted(()=>{window.addEventListener('blur',release);window.addEventListener('pagehide',release);document.addEventListener('visibilitychange',hidden);});
onUnmounted(()=>{release();window.removeEventListener('blur',release);window.removeEventListener('pagehide',release);document.removeEventListener('visibilitychange',hidden);});
watch(kind,()=>{message.value='';failed.value=false;});
onMounted(async()=>{
  try { const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/policy');
    const result=await response.json();modelAvailable.value.stand=response.ok && result.available;
    for(const key of ['walk','xgoduck'] as const)modelAvailable.value[key]=response.ok && !!result.models?.find((model:{kind:string;available:boolean})=>model.kind===key)?.available; }
  catch { modelAvailable.value={stand:false,walk:false,xgoduck:false}; }
});
async function run(action:'start'|'stop'|'shadow'|'disable'|'home') {
  release();
  pending.value=true;message.value='';failed.value=false;
  try {
    const response=await fetch(state.endpoint.replace(/\/$/,'')+(action==='disable'?'/api/v1/servos/disable':'/api/v1/policy/'+action),{
      method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({revision:board.data?.revision,kind:kind.value,speed:kind.value==='walk'?speed.value:0}),signal:AbortSignal.timeout(15000)});
    const result=await response.json();failed.value=!response.ok || result.state==='failed';if(action==='start' && !failed.value)session.value=result.driveSession || '';message.value=result.detail || (action==='home' && !failed.value ? '已到'+poseName.value+'并保持，未开始模型推理' : result.message);
  } catch(e) { failed.value=true;message.value='未收到操作结果，请核对实时状态：'+String(e); }
  finally { pending.value=false; }
}
</script>
<template>
  <div class="runtime-panel">
    <div class="runtime-title"><span>{{ kind==='xgoduck'?'XgoDuck':kind==='walk'?'HD1910 · v6':'HD1910 · v5' }}</span><strong>{{ kind==='xgoduck'?'按住行走 · 松开平衡':kind==='walk'?'模型行走':'模型站立维持' }}</strong><small>50 Hz 推理 · 100 Hz 反馈目标</small></div>
    <div class="model-selector"><button :class="{active:kind==='xgoduck'}" :disabled="busy" @click="kind='xgoduck'">XgoDuck</button><button :class="{active:kind==='stand'}" :disabled="busy" @click="kind='stand'">站立模型</button><button :class="{active:kind==='walk'}" :disabled="busy" @click="kind='walk'">行走模型</button><label v-if="kind==='walk'">前进 <select v-model.number="speed" :disabled="busy"><option :value="0.05">0.05 m/s</option><option :value="0.1">0.10 m/s</option><option :value="0.2">0.20 m/s</option></select></label></div>
    <p v-if="kind==='xgoduck'">先点“预备姿势”，再启动原地平衡。按住方向按钮才行走，松开回到零速度平衡；切到后台或断线也归零。停止结束推理并保持，全部失能卸力。</p>
    <p v-else>开始时先匀速分段到所选模型姿势，再持续{{ kind==='walk'?'按前进速度行走':'调整站姿' }}。“{{ poseName }}”以50Hz匀速分段到初始姿势并保持。停止结束推理并保持；全部失能立即卸力。</p>
    <div class="runtime-buttons">
      <button :disabled="!available || !live || !board.ready || busy" @click="run('start')">▷ {{ kind==='xgoduck'?'启动原地平衡':kind==='walk'?'开始模型行走':'开始模型站立' }}</button>
      <button :disabled="!available || !live || !board.ready || busy" @click="run('home')">{{ poseName }}</button>
      <button :disabled="!running || pending" @click="run('stop')">□ 停止并保持</button>
      <button :disabled="!live || pending" @click="run('disable')">全部失能</button>
      <button :disabled="!available || !live || busy" @click="run('shadow')">只读推理检查</button>
    </div>
    <div v-if="kind==='xgoduck'" class="hold-directions">
      <button v-for="direction in directions" :key="direction.name" :disabled="!canDrive" :class="{held:pressed===direction.name}"
        @pointerdown.prevent="press($event,direction.name,direction.twist)" @pointerup="release" @pointercancel="release" @lostpointercapture="release" @contextmenu.prevent>{{ direction.name }} · 按住</button>
    </div>
    <div class="runtime-metrics"><span>实际发令 <b>{{ control?.action==='policy' && control.commandHz!=null ? control.commandHz.toFixed(1)+' Hz' : '—' }}</b></span><span>推理 <b>{{ control?.action==='policy' && control.inferenceMs!=null ? control.inferenceMs.toFixed(2)+' ms' : '—' }}</b></span></div>
    <div class="runtime-status" :class="{failed:failed || control?.state==='failed'}" role="status">{{ (control?.state==='failed' ? control.message : '') || message || (control?.action==='policy' ? control.message : '') || (available?(kind==='walk'?'v6':'v5')+' 已部署 · 等待开始':'模型未就绪') }}</div>
    <small>{{ kind==='xgoduck'?'XgoDuck 原版模型 · 指令速度不等于实际速度，实机效果待验证。':kind==='walk'?'v6仿真可迈步；低头/走偏待优化，实机未验收。':'XgoDuck运行配置：P6/D20 · 动作平滑0.45。' }}</small>
  </div>
</template>
<style scoped>
.runtime-panel{grid-column:1/-1;height:360px;box-sizing:border-box;display:flex;flex-direction:column;gap:11px;padding:15px 8px;color:#426556}.runtime-title{display:flex;align-items:center;gap:12px;flex-wrap:wrap}.runtime-title>span{font-size:10px;border-radius:8px;background:#dcebe1;padding:5px 9px}.runtime-title strong{font-size:17px}.runtime-title small{font-size:10px;color:#7b9185}.runtime-panel p{font-size:12px;margin:0;line-height:1.7}.runtime-buttons{display:grid;grid-template-columns:1fr 1fr;gap:9px}.runtime-buttons button{border:1px solid #bdd0c3;border-radius:9px;background:#f5faf6;color:#365843;padding:10px;font-size:12px;cursor:pointer}.runtime-buttons button:first-child{background:#4a765b;color:white}.runtime-buttons button:disabled{opacity:.45;cursor:default}.runtime-metrics{display:flex;gap:25px;font-size:11px}.runtime-status{font-size:11px;min-height:30px;line-height:1.5;overflow:auto}.runtime-status.failed{color:#a84d40}.runtime-panel>small{font-size:10px;color:#87978c;margin-top:auto}@media(max-height:820px){.runtime-panel{height:330px;gap:8px;padding:9px 6px}}
.model-selector{display:flex;align-items:center;gap:7px;font-size:11px}.model-selector button,.model-selector select{border:1px solid #bdd0c3;border-radius:6px;background:#f5faf6;color:#365843;padding:4px 7px;font-size:11px}.model-selector button.active{background:#dcebe1;font-weight:600}.model-selector label{margin-left:auto}.model-selector button:disabled{opacity:.45}
.runtime-panel{height:auto;min-height:360px}.hold-directions{display:grid;grid-template-columns:repeat(3,1fr);gap:6px}.hold-directions button{touch-action:none;user-select:none;border:1px solid #bdd0c3;border-radius:7px;background:#f5faf6;color:#365843;padding:9px;font-size:12px}.hold-directions button.held{background:#4a765b;color:white}.hold-directions button:disabled{opacity:.45}
</style>
