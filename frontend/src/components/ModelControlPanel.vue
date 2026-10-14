<script setup lang="ts">
import {computed, onMounted, ref} from 'vue';
import ModelRuntime from './ModelRuntime.vue';
import {useTelemetry} from '../store';
const state=useTelemetry();
const gamepadDebug=ref(true),modeLoading=ref(true),modeError=ref('');
const hasGamepad=computed(()=>!!state.gamepadFeedback);
const boardOnline=computed(()=>state.connection==='在线');
const connectionClass=computed(()=>!boardOnline.value?'offline':hasGamepad.value?'connected':'waiting');
const connectionText=computed(()=>!boardOnline.value?'主板离线':hasGamepad.value?'手柄已连接':'等待手柄输入');
async function readMode(){
  try{const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/gamepad/mode');const data=await response.json();if(!response.ok)throw new Error(data.detail||'读取模式失败');gamepadDebug.value=data.debug===true;}
  catch(e){modeError.value='调试模式读取失败：'+String(e);}finally{modeLoading.value=false;}
}
async function toggleMode(){
  if(modeLoading.value)return;modeLoading.value=true;modeError.value='';
  try{const response=await fetch(state.endpoint.replace(/\/$/,'')+'/api/v1/gamepad/mode',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({debug:!gamepadDebug.value}),signal:AbortSignal.timeout(5000)});const data=await response.json();if(!response.ok)throw new Error(data.detail||'切换失败');gamepadDebug.value=data.debug===true;}
  catch(e){modeError.value='调试模式切换失败：'+String(e);}finally{modeLoading.value=false;}
}
onMounted(()=>void readMode());
</script>

<template>
  <section class="model-controls" aria-label="实机模型控制">
    <div class="control-heading">
      <div class="heading-title"><span class="control-symbol" aria-hidden="true">◇</span><strong>模型手柄</strong></div>
      <div class="status-tools">
        <span class="connection-state" :class="connectionClass" role="status"><i aria-hidden="true"/><span>{{ connectionText }}</span></span>
        <button class="debug-toggle" :class="{enabled:!gamepadDebug}" :disabled="modeLoading" :aria-pressed="gamepadDebug" @click="toggleMode">
          {{ modeLoading?'读取模式…':gamepadDebug?'调试开 · 仅显示':'控制开 · 允许指令' }}
        </button>
      </div>
      <span class="backend-tag">实机模型</span>
    </div>
    <p class="mode-hint" :class="{active:!gamepadDebug}">{{ gamepadDebug?'调试模式开启：实体按键只高亮，不发送控制指令':'调试模式已关闭：实体手柄控制已启用' }}</p>
    <p v-if="modeError" class="mode-error" role="alert">{{ modeError }}</p>
    <ModelRuntime />
  </section>
</template>

<style scoped>
.model-controls{box-sizing:border-box;min-width:0;width:100%;padding:12px 14px;border:1px solid #c8dace;border-radius:12px;background:linear-gradient(145deg,#f7faf7,#eaf2eb);color:#365a44}
.control-heading{display:grid;grid-template-columns:1fr auto 1fr;align-items:center;gap:8px;padding-bottom:8px;border-bottom:1px solid #d6e3d9}
.heading-title,.status-tools{display:flex;align-items:center;gap:8px}.heading-title{justify-self:start}.status-tools{justify-self:center;justify-content:center;flex-wrap:wrap}
.control-symbol{color:#668c71;font-size:18px}.control-heading strong{font-size:14px}
.backend-tag{font-size:10px;padding:4px 8px;border-radius:6px;background:#dcebe1;color:#466b52}
.backend-tag{justify-self:end}
.connection-state{display:inline-flex;align-items:center;gap:6px;font-size:12px;font-weight:650;white-space:nowrap}.connection-state i{width:9px;height:9px;border-radius:50%;background:#9aa7a0;box-shadow:0 0 0 3px #9aa7a022}.connection-state.connected i{background:#26945b;box-shadow:0 0 0 3px #26945b22}.connection-state.waiting i{background:#d29a21;box-shadow:0 0 0 3px #d29a2122}.connection-state.offline i{background:#c24b4b;box-shadow:0 0 0 3px #c24b4b22}.connection-state.connected{color:#247444}.connection-state.waiting{color:#936810}.connection-state.offline{color:#a43e3e}
.debug-toggle{border:1px solid #7b91a2;border-radius:7px;background:#e8eff4;color:#36556c;font-size:11px;font-weight:650;padding:6px 9px;cursor:pointer}.debug-toggle.enabled{border-color:#ce9a53;background:#fff0d8;color:#85540b}.debug-toggle:disabled{opacity:.55;cursor:wait}
.mode-hint{margin:6px 0 0;font-size:10px;color:#536c5a;text-align:center}.mode-hint.active{color:#85540b}.mode-error{margin:4px 0;color:#a43e3e;font-size:11px;text-align:center}
@media(max-width:620px){.control-heading{grid-template-columns:1fr auto;row-gap:8px}.status-tools{grid-row:2;grid-column:1/-1;justify-self:center}.backend-tag{grid-column:2;grid-row:1}.mode-hint{line-height:1.4}}
</style>
