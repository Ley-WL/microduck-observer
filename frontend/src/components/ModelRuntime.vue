<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { driveLease } from '../driveLease';
import WalkWheel from './WalkWheel.vue';
import { holdToWalk, type Twist } from '../holdToWalk';
import { useTelemetry } from '../store';
import { useBoardCalibration } from '../boardCalibration';
const state=useTelemetry(),board=useBoardCalibration();
const modelAvailable=ref(false),wheelSpeed=ref(0.1);
const lease=driveLease(sessionStorage,state.endpoint);
const session=ref(lease.session),pressed=ref(''), skillQueued=ref(false), commandState=ref('');
const skills=[{key:'sit',name:'坐下'},{key:'standup',name:'站起'},{key:'getup',name:'倒地起身'},{key:'pick',name:'拾取'},{key:'roulade',name:'翻滚'}];
const skillAvailable=ref<Record<string,boolean>>({});

const canStand=computed(()=>live.value && control.value?.state==='policy' && control.value?.activeSkill==='sitstand_sit' && control.value?.skillProgressSeconds>=(control.value?.sitSettleSeconds??4) && !control.value?.feedbackHolding && !!session.value && control.value?.driveSession===session.value && !pending.value);
const available=computed(()=>modelAvailable.value);
const poseName='坐下预备';
const pending=ref(false),message=ref(''),failed=ref(false);
const control=computed(()=>state.system?.data.servoControl);
const running=computed(()=>control.value?.action==='policy' && ['preflight','moving','policy'].includes(control.value?.state));
const busy=computed(()=>pending.value || ['preflight','moving','policy','disabling'].includes(control.value?.state));
const live=computed(()=>state.connection==='在线' && state.joints?.source==='hardware' && state.jointsAge<500);
const canDrive=computed(()=>live.value && control.value?.state==='policy' && control.value?.kind==='xgoduck' && !control.value?.feedbackHolding && !skillQueued.value && (!control.value?.activeSkill || control.value.activeSkill==='xgoduck') && !!session.value && control.value?.driveSession===session.value && !pending.value);
const canRecover=computed(()=>live.value && control.value?.state==='policy' && control.value?.kind==='xgoduck' && ['xgoduck','xgoduck_pick','xgoduck_roulade','sitstand_sit','sitstand_stand'].includes(control.value?.activeSkill) && !control.value?.feedbackHolding && !!session.value && control.value?.driveSession===session.value && !pending.value);
let acknowledgedSequence=0;
watch(session,()=>{acknowledgedSequence=0;commandState.value='';});
const hold=holdToWalk(async(twist,sequence)=>{
  const requestSession=session.value;
  const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/policy/command',{
    method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({session:requestSession,sequence,twist}),keepalive:true,signal:AbortSignal.timeout(1000)});
  if(!response.ok){const result=await response.json();throw new Error(result.detail || '行走指令未接受');}
  if(requestSession!==session.value || sequence<acknowledgedSequence)return;acknowledgedSequence=sequence;
  commandState.value=twist.some(v=>v!==0)?'已接受：前后 '+twist[0].toFixed(2)+' · 转向 '+twist[2].toFixed(2)+' rad/s':'零速度指令已接受';
},{nextSequence:()=>lease.nextSequence(),onError:error=>{pressed.value='';failed.value=true;message.value='行走指令失败：'+String(error);commandState.value='指令被拒绝 / 未收到响应';}});
watch(()=>[control.value?.activeSkill,control.value?.state],()=>{if(control.value?.activeSkill!=='xgoduck' || control.value?.state!=='policy')skillQueued.value=false;});
async function extra(action:string,angleDeg?:number){
  release();pending.value=true;failed.value=false;
  try{
    const start=['getup','standup','sit'].includes(action) && !running.value;
    const path=start?'start':action==='quack'?'quack':action==='mouth'?'mouth':'skill';
    const body=start?{kind:action==='sit'?'sitstand_sit':action==='standup'?'sitstand_stand':'xgoduck_getup',speed:0,revision:board.data?.revision}:angleDeg!=null && !running.value?{id:34,angleDeg,revision:board.data?.revision}:{session:session.value,sequence:hold.nextSequence(),skill:action,angleDeg};
    const url=action==='quack' && !running.value?'/api/v1/servos/quack':angleDeg!=null && !running.value?'/api/v1/servos/angle':'/api/v1/policy/'+path;
    if(action==='quack' && !running.value)Object.assign(body,{revision:board.data?.revision});
    const response=await fetch(state.endpoint.replace(/\/$/,'')+url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(15000)});
    const result=await response.json();if(!response.ok)throw new Error(result.detail || '操作未接受');
    if(start){session.value=result.driveSession || '';lease.remember(session.value);}
    if(angleDeg==null && action!=='quack')skillQueued.value=control.value?.state==='policy' && control.value?.activeSkill==='xgoduck';
    message.value=action==='quack'?'嘎一下已请求':angleDeg!=null?'嘴部目标已发送':action==='sit'?'坐下已请求，模型将持续维持坐姿':'动作已请求，完成后回到原地平衡';
  }catch(e){failed.value=true;message.value=String(e);}finally{pending.value=false;}
}
const driveReason=computed(()=>{
  if(!live.value)return '实机连接或关节反馈未就绪';
  if(pending.value)return '等待操作完成';
  if(control.value?.state==='failed')return control.value.message || '模型故障，轮盘锁定';
  if(control.value?.state==='holding')return '模型已停止：请站起后行走';
  if(control.value?.state!=='policy')return '先坐下预备，再站起';
  if(control.value?.kind!=='xgoduck')return '当前运行其他模型，请切换平衡行走';
  if(control.value?.feedbackHolding)return '反馈暂缺，等待恢复后重新按住';
  if(skillQueued.value || (control.value?.activeSkill && control.value.activeSkill!=='xgoduck'))return control.value?.activeSkill==='sitstand_sit'?'坐姿维持中：点站起后恢复行走':'动作执行中，完成后再行走';
  if(!session.value || control.value?.driveSession!==session.value)return '当前平衡由其他页面或App启动：停止保持后，在本页重新启动';
  return '';
});
function release(){pressed.value='';hold.release();}
function wheel(twist:Twist){
  if(!canDrive.value)return;
  if(pressed.value==='轮盘')hold.update(twist);
  else { pressed.value='轮盘';hold.press(twist); }
}
function hidden(){if(document.hidden)release();}
watch(canDrive,value=>{if(!value)release();});
onMounted(()=>{window.addEventListener('blur',release);window.addEventListener('pagehide',release);document.addEventListener('visibilitychange',hidden);});
onUnmounted(()=>{release();window.removeEventListener('blur',release);window.removeEventListener('pagehide',release);document.removeEventListener('visibilitychange',hidden);});
onMounted(async()=>{
  try { const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/policy');
    const result=await response.json();modelAvailable.value=response.ok && !!result.models?.find((model:{kind:string;available:boolean})=>model.kind==='xgoduck')?.available;
    for(const skill of skills)skillAvailable.value[skill.key]=!!result.models?.find((model:{kind:string;available:boolean})=>model.kind===(skill.key==='sit'?'sitstand_sit':skill.key==='standup'?'sitstand_stand':'xgoduck_'+skill.key))?.available;
    }
  catch { modelAvailable.value=false; }
});
async function run(action:'start'|'stop'|'shadow'|'disable'|'home') {
  if(action==='home'){await extra('sit');return;}
  if(action==='start'){await extra('standup');return;}
  release();
  pending.value=true;message.value='';failed.value=false;
  try {
    const response=await fetch(state.endpoint.replace(/\/$/,'')+(action==='disable'?'/api/v1/servos/disable':'/api/v1/policy/'+action),{
      method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({revision:board.data?.revision,kind:'xgoduck',speed:0}),signal:AbortSignal.timeout(15000)});
    const result=await response.json();failed.value=!response.ok || result.state==='failed';message.value=result.detail || result.message;
  } catch(e) { failed.value=true;message.value='未收到操作结果，请核对实时状态：'+String(e); }
  finally { pending.value=false; }
}
</script>
<template>
  <div class="runtime-panel">
    <div class="runtime-overview">
    <div class="runtime-title"><span>平衡行走</span><strong>按住行走 · 松开平衡</strong><small>实机模型</small></div>
    </div>
    <p>坐下预备 → 站起 → 按住行走；松开平衡，坐下后点站起恢复。</p>
    <div class="runtime-buttons">
      <button :disabled="!skillAvailable.standup || !(canStand || (live && board.ready && !busy))" @click="run('start')">▷ 站起</button>
      <button :disabled="!available || !live || !board.ready || busy" @click="run('home')">{{ poseName }}</button>
      <button :disabled="!running || pending" @click="run('stop')">□ 停止并保持</button>
      <button :disabled="!live || pending" @click="run('disable')">全部失能</button>
      <button :disabled="!available || !live || busy" @click="run('shadow')">只读推理检查</button>
    </div>
    <div class="walk-speed-row">
      <label>前后档位 <input v-model.number="wheelSpeed" type="range" min="0.02" max="0.40" step="0.01" aria-label="轮盘前后速度" /><output>{{ wheelSpeed.toFixed(2) }}</output></label>
      <button v-for="value in [0.05,0.1,0.2,0.3,0.4]" :key="value" :class="{selected:wheelSpeed===value}" @click="wheelSpeed=value">{{ value.toFixed(2) }}</button>
      <small>模型指令速度 · 非实测速度</small>
    </div>
    <div class="runtime-interaction">
    <p class="drive-result" role="status">{{ driveReason || commandState || '轮盘已就绪 · 按住拖动行走' }}</p>
    <WalkWheel :speed="wheelSpeed" :disabled="!canDrive" @drive="wheel" @release="release" />
    <div class="hold-directions">
      <button v-for="skill in skills" :key="skill.key" :disabled="!skillAvailable[skill.key] || !(canDrive || (skill.key==='getup' && canRecover) || (skill.key==='standup' && canStand) || (['getup','standup'].includes(skill.key) && live && board.ready && !busy))" @click="extra(skill.key)">{{ skill.name }}</button>
      <button :disabled="!(canDrive || (live && board.ready && !busy))" @click="extra('quack')" title="张闭嘴一次，同时叫一声">嘎一下</button>
      <span>{{ control?.activeSkill==='xgoduck_getup'?'起身中':control?.activeSkill==='xgoduck_pick'?'拾取中':control?.activeSkill==='xgoduck_roulade'?'翻滚中':'' }}</span>
    </div>
    </div>
    <div class="runtime-metrics"><span>模型输入 <b>{{ control?.commandTwist?.[0]?.toFixed(2) ?? '—' }} / {{ control?.commandTwist?.[2]?.toFixed(2) ?? '—' }}（m/s · rad/s）</b></span><span>实际发令 <b>{{ control?.action==='policy' && control.commandHz!=null ? control.commandHz.toFixed(1)+' Hz' : '—' }}</b></span><span>推理 <b>{{ control?.action==='policy' && control.inferenceMs!=null ? control.inferenceMs.toFixed(2)+' ms' : '—' }}</b></span></div>
    <div class="runtime-status" :class="{failed:failed || control?.state==='failed'}" role="status">{{ (control?.state==='failed' ? control.message : '') || message || (control?.action==='policy' ? control.message : '') || (available?'平衡行走模型就绪 · 等待开始':'模型未就绪') }}</div>
  </div>
</template>
<style scoped>
.runtime-panel{grid-column:1/-1;box-sizing:border-box;display:flex;flex-direction:column;gap:7px;padding:8px 0;color:#426556;min-width:0}
.runtime-overview{display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap}
.runtime-title{display:flex;align-items:center;gap:8px}.runtime-title>span{font-size:10px;border-radius:6px;background:#dcebe1;padding:4px 6px}.runtime-title strong{font-size:14px}.runtime-title small{font-size:10px;color:#7b9185}
.runtime-panel p{font-size:11px;margin:0;line-height:1.5}
.runtime-buttons{display:grid;grid-template-columns:repeat(5,minmax(0,1fr));gap:6px}.runtime-buttons button{min-height:36px;border:1px solid #bdd0c3;border-radius:7px;background:#f5faf6;color:#365843;padding:6px 4px;font-size:11px;cursor:pointer}.runtime-buttons button:first-child{background:#4a765b;color:white}.runtime-buttons button:disabled{opacity:.45;cursor:default}
.walk-speed-row{display:flex;align-items:center;gap:6px;flex-wrap:wrap;font-size:11px}.walk-speed-row label{display:flex;align-items:center;gap:7px}.walk-speed-row input{width:130px;accent-color:#4a765b}.walk-speed-row output{min-width:65px;font-variant-numeric:tabular-nums}.walk-speed-row button{padding:4px 6px;border:1px solid #bdd0c3;border-radius:5px;background:#f5faf6;color:#365843}.walk-speed-row button.selected{background:#dcebe1;font-weight:600}.walk-speed-row small{margin-left:auto;font-size:9px;color:#7b9185}
.runtime-interaction{display:grid;grid-template-columns:minmax(290px,1fr) minmax(0,1fr);grid-template-rows:auto 1fr;gap:6px 14px;align-items:center;border-top:1px solid #dae4de;padding-top:6px}
.runtime-interaction .drive-result{grid-column:2;grid-row:1;font-size:10px;overflow-wrap:anywhere}
.runtime-interaction .wheel-control{grid-column:1;grid-row:1/3;padding:0;gap:12px;flex-wrap:nowrap}
.hold-directions{grid-column:2;grid-row:2;display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:6px;align-content:start}.hold-directions button{min-height:36px;touch-action:none;user-select:none;border:1px solid #bdd0c3;border-radius:7px;background:#f5faf6;color:#365843;padding:6px 4px;font-size:11px}.hold-directions button:disabled{opacity:.45}.hold-directions span{grid-column:1/-1;font-size:10px}
.runtime-metrics{display:flex;gap:20px;font-size:10px}.runtime-status{font-size:11px;line-height:1.5;overflow-wrap:anywhere}.runtime-status.failed{color:#a84d40}.runtime-panel>small{font-size:9px;color:#87978c}
@media(max-width:1600px){.runtime-title small{display:none}.runtime-interaction{grid-template-columns:minmax(270px,1fr) minmax(0,1fr);gap:6px 8px}}
@media(max-width:1100px){.runtime-interaction{grid-template-columns:1fr}.runtime-interaction .wheel-control{grid-row:2;grid-column:1}.runtime-interaction .drive-result{grid-column:1;grid-row:1}.hold-directions{grid-column:1;grid-row:3}.runtime-buttons{grid-template-columns:repeat(3,minmax(0,1fr))}}
</style>
