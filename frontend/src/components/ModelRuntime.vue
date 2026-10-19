<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { driveLease } from '../driveLease';
import ModelGamepad from './ModelGamepad.vue';
import { gamepadInput } from '../gamepadInput';
import type { Twist } from '../holdToWalk';
import { useTelemetry } from '../store';
import { useBoardCalibration } from '../boardCalibration';
const state=useTelemetry(),board=useBoardCalibration();
const physicalResult=computed(()=>{
  for(let i=state.logs.length-1;i>=0;i--){const log=state.logs[i];if(log.data.module==='gamepad')return '手柄最近事件：'+String(log.data.message||'');}
  return '';
});
const modelAvailable=ref(false),headAvailable=ref(false),wheelSpeed=ref(0.2);
const rearHook=ref(false);
const lease=driveLease(sessionStorage,state.endpoint);
const session=ref(lease.session),pressed=ref(''), skillQueued=ref(false), commandState=ref('');
const skills=[{key:'flamingo',name:'火烈鸟'},{key:'dance',name:'十拍鸭子舞'},{key:'sit',name:'坐下'},{key:'standup',name:'站起'},{key:'getup',name:'倒地起身'},{key:'pick',name:'拾取'},{key:'roulade',name:'翻滚'},{key:'kick_left',name:'左脚踢球'},{key:'kick_right',name:'右脚踢球'}];
const skillAvailable=ref<Record<string,boolean>>({});
const nearbyPads=ref<{address:string;name:string;likelyController:boolean}[]>([]), bluetoothBusy=ref(false), bluetoothMessage=ref('');
const visibleBluetoothDevices=computed(()=>nearbyPads.value.filter(device=>{const name=device.name.toLocaleLowerCase();return name.includes('xbox')||name.includes('wireless controller')||name.includes('gamesir');}));
async function scanBluetooth(){bluetoothBusy.value=true;bluetoothMessage.value='正在扫描主板附近的蓝牙设备…';nearbyPads.value=[];try{const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/gamepad/bluetooth/scan',{method:'POST',headers:{'Content-Type':'application/json'},body:'{}',signal:AbortSignal.timeout(18000)});const data=await response.json();if(!response.ok)throw new Error(data.detail||'扫描失败');nearbyPads.value=data.devices||[];const controllers=visibleBluetoothDevices.value.length;bluetoothMessage.value=controllers?`发现 ${controllers} 个疑似手柄，请核对后连接。`:nearbyPads.value.length?`发现 ${nearbyPads.value.length} 台设备，但没有名称匹配 Xbox、Wireless Controller 或 GameSir 的设备。`:'没有发现设备。请让手柄进入蓝牙配对模式后重试。';}catch(e){bluetoothMessage.value=String(e);}finally{bluetoothBusy.value=false;}}
async function pairBluetooth(address:string){bluetoothBusy.value=true;bluetoothMessage.value='正在配对、信任并连接所选设备…';try{const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/gamepad/bluetooth/connect',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({address}),signal:AbortSignal.timeout(55000)});const data=await response.json();if(!response.ok)throw new Error(data.detail||'连接失败');bluetoothMessage.value=data.message;}catch(e){const detail=String(e);bluetoothMessage.value=/AuthenticationFailed/i.test(detail)?'蓝牙配对认证未完成。主板没有保存这只手柄的配对；请断开它与手机、电脑或接收器的连接，关机后按 B + Home 进入 PC 蓝牙模式，确认指示灯快速闪烁，再立即扫描并配对。':/ConnectionAttemptFailed|Device not available/i.test(detail)?'扫描到手柄，但它目前没有接受连接。请确保手柄指示灯快速闪烁；若慢闪，按住 Home + Screenshot 约 2 秒直到快闪，然后马上重新扫描并配对。也请确认它没有连着手机或无线接收器。':detail;}finally{bluetoothBusy.value=false;}}

const canStand=computed(()=>live.value && control.value?.state==='policy' && control.value?.activeSkill==='sitstand_sit' && (control.value?.standAllowed??(control.value?.skillProgressSeconds>=(control.value?.sitSettleSeconds??4))) && !control.value?.feedbackHolding && !!session.value && control.value?.driveSession===session.value && !pending.value);
const available=computed(()=>modelAvailable.value);
const resetSticks=ref(0);
const seatedHead=computed(()=>live.value && control.value?.state==='policy' && control.value?.activeSkill==='sitstand_sit' && control.value?.skillProgressSeconds>=control.value?.sitSettleSeconds && !control.value?.feedbackHolding && !pending.value && !!session.value && control.value?.driveSession===session.value);
const canHead=computed(()=>(canDrive.value || seatedHead.value) && headAvailable.value && control.value?.runtimeProfile==='kissqy-sync50-v1');
const demoOwned=computed(()=>!!control.value?.demo?.active && !!session.value && control.value?.driveSession===session.value);
const allowed=computed(()=>({
  demo:live.value && !pending.value && (demoOwned.value || (canDrive.value && !!control.value?.demo?.ready)),
  standup:!!skillAvailable.value.standup && (canStand.value || (live.value && board.ready && !busy.value)),
  sit:!!skillAvailable.value.sit && (canDrive.value || (live.value && board.ready && !busy.value)),
  getup:!!skillAvailable.value.getup && (canDrive.value || canRecover.value || (live.value && board.ready && !busy.value)),
  flamingo:!!skillAvailable.value.flamingo && (canDrive.value || (live.value && control.value?.state==='policy' && control.value?.activeSkill==='flamingo_hold' && !control.value?.feedbackHolding && !pending.value && !!session.value && control.value?.driveSession===session.value)),
  dance:!!skillAvailable.value.dance && canDrive.value,
  pick:!!skillAvailable.value.pick && canDrive.value,roulade:!!skillAvailable.value.roulade && canDrive.value,
  kick_left:!!skillAvailable.value.kick_left && canDrive.value,kick_right:!!skillAvailable.value.kick_right && canDrive.value,
  quack:live.value && board.ready && !pending.value && !quackPending.value && (control.value?.state==='policy'
    ? control.value?.kind==='xgoduck' && !control.value?.feedbackHolding && !!session.value && control.value?.driveSession===session.value
    : !busy.value),
  stop:running.value && !pending.value,disable:state.connection==='在线',
}));
const quackPending=ref(false);
const pending=ref(false),message=ref(''),failed=ref(false);
const control=computed(()=>state.system?.data.servoControl);
const running=computed(()=>control.value?.action==='policy' && ['preflight','moving','policy'].includes(control.value?.state));
const busy=computed(()=>pending.value || ['preflight','moving','policy','disabling'].includes(control.value?.state));
const live=computed(()=>state.connection==='在线' && state.joints?.source==='hardware' && state.jointsAge<500);
const canDrive=computed(()=>live.value && control.value?.state==='policy' && control.value?.kind==='xgoduck' && !control.value?.feedbackHolding && !skillQueued.value && (!control.value?.activeSkill || control.value.activeSkill==='xgoduck') && !!session.value && control.value?.driveSession===session.value && !pending.value);
const canRecover=computed(()=>live.value && control.value?.state==='policy' && control.value?.kind==='xgoduck' && ['xgoduck','xgoduck_pick','xgoduck_roulade','kick_left','kick_right','sitstand_sit','sitstand_stand'].includes(control.value?.activeSkill) && !control.value?.feedbackHolding && !!session.value && control.value?.driveSession===session.value && !pending.value);
let acknowledgedSequence=0;
watch(session,()=>{acknowledgedSequence=0;commandState.value='';});
const hold=gamepadInput(async(input,sequence)=>{
  const {twist,headPitch,headYaw}=input;
  const requestSession=session.value;
  const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/policy/command',{
    method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({session:requestSession,sequence,twist,headPitch,headYaw}),keepalive:true,signal:AbortSignal.timeout(1000)});
  if(!response.ok){const result=await response.json();throw new Error(result.detail || '行走指令未接受');}
  if(requestSession!==session.value || sequence<acknowledgedSequence)return;acknowledgedSequence=sequence;
  commandState.value=twist.some(v=>v!==0)?'已接受：前后 '+twist[0].toFixed(2)+' · 转向 '+twist[2].toFixed(2)+' rad/s':'零速度指令已接受';
},()=>lease.nextSequence(),error=>{resetSticks.value++;pressed.value='';failed.value=true;message.value='手柄指令失败：'+String(error);commandState.value='指令被拒绝 / 未收到响应';});
watch(()=>[control.value?.activeSkill,control.value?.state],()=>{if(control.value?.activeSkill!=='xgoduck' || control.value?.state!=='policy')skillQueued.value=false;});
let demoHeartbeat:ReturnType<typeof setInterval>|undefined;
let demoRequestPending=false;
let demoFocused=true;
async function demoCommand(mode='toggle'){
  if(demoRequestPending)return;
  demoRequestPending=true;
  try{
    const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/policy/demo',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({session:session.value,sequence:hold.nextSequence(),mode}),keepalive:true,signal:AbortSignal.timeout(1000)});
    const result=await response.json();
    if(!response.ok)throw new Error(result.detail||'演示请求失败');
    if(mode!=='keepalive'){failed.value=false;message.value=result.active?'演示开始：约 28～35 秒，再按 LB 取消':'演示已取消，返回平衡';}
  }catch(e){if(mode!=='keepalive'){failed.value=true;message.value=String(e);}}
  finally{demoRequestPending=false;}
}
function leaveDemo(){demoFocused=false;release();if(demoOwned.value)void demoCommand('cancel');}
onMounted(()=>{demoHeartbeat=setInterval(()=>{if(demoOwned.value && demoFocused && !document.hidden)void demoCommand('keepalive');},120);window.addEventListener('blur',leaveDemo);window.addEventListener('pagehide',leaveDemo);});
onUnmounted(()=>{if(demoHeartbeat)clearInterval(demoHeartbeat);leaveDemo();window.removeEventListener('blur',leaveDemo);window.removeEventListener('pagehide',leaveDemo);});
async function extra(action:string,angleDeg?:number){
  if(action==='demo'){demoFocused=true;release();await demoCommand();return;}
  const independent=action==='quack';
  if(independent){if(quackPending.value)return;quackPending.value=true;}else{release();pending.value=true;}failed.value=false;
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
    message.value=action==='flamingo'?(rearHook.value?'后抬脚已请求：右脚向后抬起，到位保持3秒后自动回正；再按提前返回':'火烈鸟切换已请求：首次抬左脚保持，再按落脚'):action==='quack'?'嘎一下已请求':angleDeg!=null?'嘴部目标已发送':action==='sit'?'坐下已请求，模型将持续维持坐姿':'动作已请求，完成后回到原地平衡';
  }catch(e){failed.value=true;message.value=String(e);}finally{if(independent)quackPending.value=false;else pending.value=false;}
}
const driveReason=computed(()=>{
  if(!live.value)return '实机连接或关节反馈未就绪';
  if(pending.value)return '等待操作完成';
  if(control.value?.state==='failed')return control.value.message || '模型故障，手柄锁定';
  if(control.value?.state==='holding')return '模型已停止：请站起后行走';
  if(control.value?.state!=='policy')return '点击 A 站立，或 X 坐下预备';
  if(control.value?.kind!=='xgoduck')return '当前运行其他模型，请切换平衡行走';
  if(control.value?.feedbackHolding)return '反馈暂缺，等待恢复后重新按住';
  if(skillQueued.value || (control.value?.activeSkill && control.value.activeSkill!=='xgoduck'))return control.value?.activeSkill==='sitstand_sit'?'坐姿维持中：点站起后恢复行走':'动作执行中，完成后再行走';
  if(!session.value || control.value?.driveSession!==session.value)return '当前平衡由其他页面、App或手柄启动：停止保持后，在本页重新启动';
  return '';
});
function release(){resetSticks.value++;pressed.value='';hold.release();}
function wheel(twist:Twist|null){if(twist && !canDrive.value && !demoOwned.value)return;hold.body(twist);}
function head(value:[number,number]|null){if(value && !canHead.value && !demoOwned.value)return;hold.head(value);}
function padAction(action:string){if(!allowed.value[action as keyof typeof allowed.value])return;if(action==='stop'||action==='disable')void run(action);else void extra(action);}
watch(wheelSpeed,()=>release());
function hidden(){if(document.hidden)leaveDemo();}
watch(canDrive,value=>{if(!value)release();});
watch(canHead,value=>{if(!value)release();});
onMounted(()=>{window.addEventListener('blur',release);window.addEventListener('pagehide',release);document.addEventListener('visibilitychange',hidden);});
onUnmounted(()=>{release();window.removeEventListener('blur',release);window.removeEventListener('pagehide',release);document.removeEventListener('visibilitychange',hidden);});
onMounted(async()=>{
  try { const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/policy');
    const result=await response.json();headAvailable.value=response.ok && result.webHeadControl===true;modelAvailable.value=response.ok && !!result.models?.find((model:{kind:string;available:boolean})=>model.kind==='xgoduck')?.available;
    rearHook.value=['hd1910_rear_hook_v27','hd1910_rear_hook_native','hd1910_rear_hook_native_gated'].includes(result.models?.find((model:{kind:string})=>model.kind==='flamingo_hold')?.metadata?.runtimeKind);
    for(const skill of skills)skillAvailable.value[skill.key]=!!result.models?.find((model:{kind:string;available:boolean})=>model.kind===(skill.key==='flamingo'?'flamingo_hold':skill.key==='dance'?'dance10':skill.key==='sit'?'sitstand_sit':skill.key==='standup'?'sitstand_stand':skill.key.startsWith('kick_')?skill.key:'xgoduck_'+skill.key))?.available;
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
    <div class="runtime-overview"><div class="runtime-title"><span>网页手柄</span><strong>点击按键 · 按住摇杆</strong><small>T4 Nova Lite 布局</small></div></div>
    <div v-if="control?.state==='policy'" class="runtime-metrics" role="status"><span>近 1 秒实际推理 <b>{{ Number.isFinite(control?.cycleCosts?.inferenceHzLast1s) ? control.cycleCosts.inferenceHzLast1s.toFixed(1)+' Hz' : '—' }}</b></span></div>
    <details class="bluetooth-setup"><summary>连接实物蓝牙手柄</summary><p>手柄关机时同时短按 <b>B + Home</b> 进入蓝牙配对，再扫描。确认名称和设备后点“配对并连接”。</p><div class="bluetooth-actions"><button :disabled="bluetoothBusy" @click="scanBluetooth">{{ bluetoothBusy?'请稍候…':'扫描主板附近设备' }}</button><span role="status">{{ bluetoothMessage }}</span></div><div class="bluetooth-devices"><div v-for="device in visibleBluetoothDevices" :key="device.address" class="bluetooth-device"><span><b>{{ device.name }}</b> · {{ device.address }}<small>疑似 Xbox / GameSir 手柄</small></span><button :disabled="bluetoothBusy" @click="pairBluetooth(device.address)">配对并连接</button></div></div><small>仅列出名称包含 Xbox、Wireless Controller 或 GameSir 的设备。扫描和配对由主板执行，不要求登录后台。配对后若网页未显示实物输入，还需主板内核提供手柄输入设备。</small></details>
    <p class="drive-result" role="status">{{ state.gamepadFeedback ? (physicalResult || '实物手柄已连接 · 橙色表示输入，不代表动作执行') : (driveReason || commandState || '手柄已就绪 · 左行走，右转头') }}</p>
    <div v-if="control?.demo?.active" class="runtime-metrics" role="status"><b>演示 {{ control.demo.step }}/{{ control.demo.steps }} · {{ control.demo.name }}</b><span>{{ Math.floor(control.demo.elapsedSeconds || 0) }} 秒 · LB 再按取消，摇杆接管</span></div>
    <p v-else-if="control?.demo?.reason" role="status">{{ control.demo.reason }}</p>
    <ModelGamepad :rear-hook="rearHook" :feedback="state.gamepadFeedback" :drive="canDrive || demoOwned" :head="canHead || demoOwned" :speed="wheelSpeed" :reset="resetSticks" :allowed="allowed" :active="canDrive" @body="wheel" @head="head" @action="padAction" />
    <div class="walk-speed-row"><label>行走速度 <input v-model.number="wheelSpeed" type="range" min="0.02" max="0.40" step="0.01" aria-label="手柄行走速度" /><output>{{ wheelSpeed.toFixed(2) }} m/s</output></label><small>指令速度 · 非实测速度</small></div>
    <details class="runtime-help"><summary>按键说明与诊断</summary><p>A 站立 · Start 预留 · X 坐下 · Y 倒地起身 · B 停止保持 · Back 急停卸力 · RB 拾取 · LT 加速×2（0.2→0.4m/s）· LB 演示 / 取消（站稳后，约 28～35 秒）· RT 嘎一下（无需先站起）；十字键上舞蹈、左键火烈鸟（首次抬左脚保持，再按落脚）、右键右脚踢球、下自主行动；左右踢球执行 0.5 秒后返回平衡；舞蹈执行8秒后返回平衡，自主行动暂为预留入口。实物手柄：A 站立、长按 Y 倒地起身、模型就绪并回中后摇杆直接控制；单按 RB 拾取（模型就绪且摇杆回中）、LB 演示 / 取消、RT 嘎一下；LT按住走行速度0.4m/s（默认0.2m/s）；网页高亮仅显示实物输入，松键或超过 500ms 无新输入自动清除。先站起，再拖动摇杆；松开保持平衡，急停卸力。</p><button :disabled="!allowed.roulade" @click="padAction('roulade')">翻滚</button> <button :disabled="!available || !live || busy" @click="run('shadow')">只读推理检查</button>    <div class="runtime-metrics"><span>模型输入 <b>{{ control?.commandTwist?.[0]?.toFixed(2) ?? '—' }} / {{ control?.commandTwist?.[2]?.toFixed(2) ?? '—' }}（m/s · rad/s）</b></span><span>实际发令 <b>{{ control?.action==='policy' && control.commandHz!=null ? control.commandHz.toFixed(1)+' Hz' : '—' }}</b></span><span>推理 <b>{{ control?.action==='policy' && control.inferenceMs!=null ? control.inferenceMs.toFixed(2)+' ms' : '—' }}</b></span></div>
</details>
    <div v-if="message || control?.action==='policy' || control?.state==='failed'" class="runtime-status" :class="{failed:failed || control?.state==='failed'}" role="status">{{ (control?.state==='failed' ? control.message : '') || message || (control?.action==='policy' ? control.message : '') || (available?'平衡行走模型就绪 · 等待开始':'模型未就绪') }}</div>
  </div>
</template>
<style scoped>
.runtime-help{font-size:11px;line-height:1.7}.runtime-help summary{cursor:pointer}.runtime-help button{margin-top:6px;border:1px solid #bdd0c3;background:#f5faf6;border-radius:6px;padding:6px 10px;color:#365843}.runtime-help button:disabled{opacity:.45}
.bluetooth-setup{border:1px solid #c8dace;background:#f8fbf8;border-radius:8px;padding:8px 10px;font-size:11px}.bluetooth-setup summary{cursor:pointer;font-weight:600}.bluetooth-setup p{margin:6px 0}.bluetooth-actions,.bluetooth-device{display:flex;align-items:center;gap:8px;margin:6px 0}.bluetooth-actions span{overflow-wrap:anywhere}.bluetooth-devices{max-height:220px;overflow:auto}.bluetooth-device{justify-content:space-between;border-top:1px solid #e0e9e1;padding-top:6px}.bluetooth-device small{display:block;color:#71877a}.bluetooth-setup button{border:1px solid #bdd0c3;background:#edf5ee;border-radius:6px;padding:6px 10px;color:#365843;cursor:pointer}.bluetooth-setup button:disabled{opacity:.5}.bluetooth-setup>small{color:#72877a;line-height:1.5}
.runtime-panel{grid-column:1/-1;box-sizing:border-box;display:flex;flex-direction:column;gap:7px;padding:8px 0;color:#426556;min-width:0}
.runtime-overview{display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap}
.runtime-title{display:flex;align-items:center;gap:8px}.runtime-title>span{font-size:10px;border-radius:6px;background:#dcebe1;padding:4px 6px}.runtime-title strong{font-size:14px}.runtime-title small{font-size:10px;color:#7b9185}
.runtime-panel p{font-size:11px;margin:0;line-height:1.5}
.walk-speed-row{display:flex;align-items:center;gap:6px;flex-wrap:wrap;font-size:11px}.walk-speed-row label{display:flex;align-items:center;gap:7px}.walk-speed-row input{width:130px;accent-color:#4a765b}.walk-speed-row output{min-width:65px;font-variant-numeric:tabular-nums}.walk-speed-row button{padding:4px 6px;border:1px solid #bdd0c3;border-radius:5px;background:#f5faf6;color:#365843}.walk-speed-row button.selected{background:#dcebe1;font-weight:600}.walk-speed-row small{margin-left:auto;font-size:9px;color:#7b9185}
.runtime-metrics{display:flex;gap:20px;font-size:10px}.runtime-status{font-size:11px;line-height:1.5;overflow-wrap:anywhere}.runtime-status.failed{color:#a84d40}.runtime-panel>small{font-size:9px;color:#87978c}
.runtime-metrics{flex-wrap:wrap;gap:8px 16px}.drive-result{min-height:18px}
</style>
