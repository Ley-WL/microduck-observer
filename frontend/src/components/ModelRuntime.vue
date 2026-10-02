<script setup lang="ts">
import { computed, ref, onMounted } from 'vue';
import { useTelemetry } from '../store';
import { useBoardCalibration } from '../boardCalibration';
const state=useTelemetry(),board=useBoardCalibration();
const available=ref(false),pending=ref(false),message=ref(''),failed=ref(false);
const control=computed(()=>state.system?.data.servoControl);
const running=computed(()=>control.value?.action==='policy' && ['preflight','moving','policy'].includes(control.value?.state));
const busy=computed(()=>pending.value || ['preflight','moving','policy','disabling'].includes(control.value?.state));
const live=computed(()=>state.connection==='在线' && state.joints?.source==='hardware' && state.jointsAge<500);
onMounted(async()=>{
  try { const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/policy');
    const result=await response.json();available.value=response.ok && result.available; }
  catch { available.value=false; }
});
async function run(action:'start'|'stop'|'shadow'|'disable') {
  pending.value=true;message.value='';failed.value=false;
  try {
    const response=await fetch(state.endpoint.replace(/\/$/,'')+(action==='disable'?'/api/v1/servos/disable':'/api/v1/policy/'+action),{
      method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({revision:board.data?.revision}),signal:AbortSignal.timeout(15000)});
    const result=await response.json();failed.value=!response.ok || result.state==='failed';message.value=result.detail || result.message;
  } catch(e) { failed.value=true;message.value='未收到操作结果，请核对实时状态：'+String(e); }
  finally { pending.value=false; }
}
</script>
<template>
  <div class="runtime-panel">
    <div class="runtime-title"><span>HD1910 · v5</span><strong>模型站立维持</strong><small>50 Hz 闭环 · IMU + 14 个关节反馈</small></div>
    <p>托稳直立后开始：先过渡到模型站姿，再持续调整。停止结束推理并保持；全部失能立即卸力。</p>
    <div class="runtime-buttons">
      <button :disabled="!available || !live || !board.ready || busy" @click="run('start')">▷ 开始模型站立</button>
      <button :disabled="!running || pending" @click="run('stop')">□ 停止并保持</button>
      <button :disabled="!live || pending" @click="run('disable')">全部失能</button>
      <button :disabled="!available || !live || busy" @click="run('shadow')">只读推理检查</button>
    </div>
    <div class="runtime-metrics"><span>实际发令 <b>{{ control?.action==='policy' && control.commandHz!=null ? control.commandHz.toFixed(1)+' Hz' : '—' }}</b></span><span>推理 <b>{{ control?.action==='policy' && control.inferenceMs!=null ? control.inferenceMs.toFixed(2)+' ms' : '—' }}</b></span></div>
    <div class="runtime-status" :class="{failed:failed || control?.state==='failed'}" role="status">{{ (control?.state==='failed' ? control.message : '') || message || (control?.action==='policy' ? control.message : '') || (available?'v5 已部署 · 等待开始':'模型未就绪') }}</div>
    <small>仅站立维持；移动、坐起及其他技能仍为操作预览。</small>
  </div>
</template>
<style scoped>
.runtime-panel{grid-column:1/-1;height:278px;box-sizing:border-box;display:flex;flex-direction:column;gap:11px;padding:15px 8px;color:#426556}.runtime-title{display:flex;align-items:center;gap:12px;flex-wrap:wrap}.runtime-title>span{font-size:10px;border-radius:8px;background:#dcebe1;padding:5px 9px}.runtime-title strong{font-size:17px}.runtime-title small{font-size:10px;color:#7b9185}.runtime-panel p{font-size:12px;margin:0;line-height:1.7}.runtime-buttons{display:grid;grid-template-columns:1fr 1fr;gap:9px}.runtime-buttons button{border:1px solid #bdd0c3;border-radius:9px;background:#f5faf6;color:#365843;padding:10px;font-size:12px;cursor:pointer}.runtime-buttons button:first-child{background:#4a765b;color:white}.runtime-buttons button:disabled{opacity:.45;cursor:default}.runtime-metrics{display:flex;gap:25px;font-size:11px}.runtime-status{font-size:11px;min-height:30px;line-height:1.5;overflow:auto}.runtime-status.failed{color:#a84d40}.runtime-panel>small{font-size:10px;color:#87978c;margin-top:auto}@media(max-height:820px){.runtime-panel{height:251px;gap:8px;padding:9px 6px}}
</style>
