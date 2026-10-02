<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useTelemetry } from '../store';
import { useBoardCalibration } from '../boardCalibration';
const state = useTelemetry(), board = useBoardCalibration();
const pending = ref(false), unloading = ref(false), message = ref(''), failed = ref(false);
const dialog = ref<HTMLDialogElement>();
const confirmed = ref(false), revision = ref<number>();
let requestSequence = 0;
const live = computed(() => state.info?.capabilities?.joints && state.joints?.source === 'hardware' && state.jointsAge < 500);
const control = computed(() => state.system?.data.servoControl);
const busy = computed(() => pending.value || ['preflight', 'moving', 'policy', 'disabling'].includes(control.value?.state));
const calibrated = computed(() => board.ready && [10,11,12,13,14,20,21,22,23,24,30,31,32,33].every(id => Number.isFinite(board.data?.joints.references[String(id)])));
watch(() => state.endpoint, () => { dialog.value?.close(); message.value = ''; });
function askStand() {
  confirmed.value = false; revision.value = board.data?.revision;
  dialog.value?.showModal();
}
async function run(action: 'enable' | 'disable' | 'stand') {
  if (action === 'stand' && !confirmed.value) return;
  dialog.value?.close();
  const endpoint = state.endpoint.replace(/\/$/, '');
  const boot = state.joints?.bootId;
  const requestId = ++requestSequence;
  if (action === 'disable') unloading.value = true; else pending.value = true;
  failed.value = false; message.value = '';
  try {
    const response = await fetch(endpoint + '/api/v1/servos/' + action, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(action === 'stand' ? { confirmCalibration: true, revision: revision.value } : {}),
      signal: AbortSignal.timeout(15000),
    });
    const result = await response.json();
    if (requestId !== requestSequence || endpoint !== state.endpoint.replace(/\/$/, '') || boot !== state.joints?.bootId) return;
    if (!response.ok) {
      failed.value = true; message.value = result.detail || '操作被服务拒绝';
      return;
    }
    failed.value = result.state === 'failed'; message.value = result.message;
  } catch (e) {
    if (requestId === requestSequence && endpoint === state.endpoint.replace(/\/$/, '')) {
      failed.value = true;
      message.value = '未收到有效操作结果：' + (e instanceof Error ? e.message : String(e)) + '；请核对实时扭矩状态，勿重复发起动作。';
    }
  } finally {
    if (action === 'disable') unloading.value = false; else pending.value = false;
  }
}
</script>

<template>
  <div class="servo-actions">
    <div class="servo-action-buttons">
      <button class="button compact" :disabled="!live || busy" @click="run('enable')">全部使能</button>
      <button class="button compact unload-button" :disabled="!state.info?.capabilities?.joints || unloading" @click="run('disable')">{{ unloading ? '正在失能…' : '全部失能' }}</button>
      <button class="button compact stand-button" :disabled="!live || !calibrated || busy" @click="askStand">静态站姿 <small>≤3s</small></button>
    </div>
    <div class="servo-action-status" :class="{ failed }" role="status">
      <template v-if="busy && control?.state === 'moving'">站姿过渡 {{ Math.round((control.progress || 0) * 100) }}% · 剩余 {{ control.remainingSeconds }}s</template>
      <template v-else>{{ message || (control?.action !== 'angle' ? control?.message : '') || '使能锁住当前位置 · 失能释放扭矩' }}</template>
      <span v-if="control?.commandHz != null"> · 发令 {{ control.commandHz.toFixed(1) }}Hz / 目标50Hz · 最大间隔 {{ control.commandMaxGapMs }}ms</span>
    </div>
    <progress v-if="control?.state === 'moving'" :value="control.progress || 0" max="1" aria-label="站姿过渡进度" />
    <Teleport to="body">
      <dialog ref="dialog" class="stand-dialog">
        <h2>确认位置标定 · 站姿保持</h2>
        <p>请托住机器人并留出关节活动空间。双腿与头颈共 14 个关节将平滑调整到官方站姿角度并保持使能，嘴部不变。</p>
        <p>标定版本 {{ revision }} · 预计过渡 2.2 秒，3 秒内检查到位：各关节误差≤5°并保持100ms。硬件故障或超时将失能。这是静态姿势保持，没有自动平衡。</p>
        <label><input v-model="confirmed" type="checkbox" />我已完成位置标定，确认 3D 关节与实物对应正确，并已托住机器人</label>
        <div><button class="button" @click="dialog?.close()">取消</button><button class="button stand-button" :disabled="!confirmed || busy || !live" @click="run('stand')">确认并保持站姿</button></div>
      </dialog>
    </Teleport>
  </div>
</template>

<style scoped>
.servo-actions{padding:0 12px 8px;flex-shrink:0}
.servo-action-buttons{display:flex;gap:6px}
.servo-action-buttons button{flex:1;white-space:nowrap}
.unload-button{border-color:#c9a685;color:#916440;background:#fcf6ec}
.stand-button{background:#345d49;color:#fff;border-color:#345d49}
.stand-button small{font-size:9px;opacity:.7;margin-left:3px}
.servo-action-status{font-size:10px;color:#75876b;margin-top:7px;line-height:1.4;height:28px;overflow:auto;overflow-wrap:anywhere}
.servo-action-status.failed{color:#ae533e}
progress{width:100%;height:4px;accent-color:#54795a;display:block;margin-top:5px}
.stand-dialog{max-width:480px;width:calc(100% - 40px);padding:24px;border:1px solid #bdcdb8;border-radius:14px;background:#f8faf4;color:#334c39;box-shadow:0 20px 80px #102b2240}
.stand-dialog::backdrop{background:#102b2288}
.stand-dialog h2{font-size:18px;margin:0 0 16px}
.stand-dialog p{font-size:13px;line-height:1.8;color:#6a7c63}
.stand-dialog label{display:flex;align-items:flex-start;gap:8px;font-size:13px;line-height:1.7;margin:20px 0}
.stand-dialog>div{display:flex;justify-content:flex-end;gap:10px}
</style>
