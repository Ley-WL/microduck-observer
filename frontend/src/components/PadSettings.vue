<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { useTelemetry } from '../store';
import { registryIdle } from '../modelRegistry';
import { actionLabels, buttonLabels, padButtons, requestBindings, type Bindings, type PadBindingInfo } from '../padBindings';
const state = useTelemetry();
const info = ref<PadBindingInfo>(), draft = ref<Bindings>({});
const busy = ref(false), message = ref(''), error = ref(false);
let generation = 0;
const online = computed(() => state.connection === '在线');
const idle = computed(() => registryIdle(state.system?.data?.servoControl?.state || (state.info?.source === 'simulation' ? 'idle' : undefined)));
const dirty = computed(() => !!info.value && padButtons.some(key => draft.value[key] !== info.value!.config.bindings[key]));
const editable = computed(() => online.value && idle.value && !busy.value && !!info.value);
async function load() {
  const token = ++generation; info.value = undefined; draft.value = {}; message.value = '';
  if (!online.value) return;
  busy.value = true;
  try { const result = await requestBindings(state.endpoint); if (token !== generation) return; info.value = result; draft.value = { ...result.config.bindings }; }
  catch (e) { if (token === generation) { error.value = true; message.value = String(e); } }
  finally { if (token === generation) busy.value = false; }
}
watch(() => [state.endpoint, online.value], () => void load(), { immediate: true });
onUnmounted(() => { generation++; });
async function save() {
  if (!editable.value || !info.value) return;
  const token = generation, endpoint = state.endpoint;
  busy.value = true; error.value = false; message.value = '正在保存…';
  try {
    const result = await requestBindings(endpoint, { revision: info.value.config.revision, bindings: { ...draft.value } });
    if (token !== generation) return;
    info.value.config = result.config; draft.value = { ...result.config.bindings }; message.value = result.message;
  } catch (e) { if (token === generation) { error.value = true; message.value = String(e); } }
  finally { if (token === generation) busy.value = false; }
}
</script>
<template>
  <section class="pad-settings">
    <div class="eyebrow">CONTROLLER SETTINGS</div><h1>手柄自定义控制</h1>
    <p>网页虚拟手柄和板端实体手柄共用一套动作映射。保存不会触发动作，重启后保留。</p>
    <p v-if="!online">服务离线，连接后可读取配置。</p>
    <p v-else-if="!idle">请先停止模型及其他控制任务，再修改按键。</p>
    <div class="settings-grid">
      <section class="settings-card"><h2>动作按键 <small v-if="dirty">有未保存修改</small></h2>
        <div v-for="key in padButtons" :key="key" class="binding-row" :class="{ pressed: state.gamepadFeedback?.buttons[key] }">
          <label :for="'binding-' + key">{{ buttonLabels[key] }}<i v-if="state.gamepadFeedback?.buttons[key]">按下</i></label>
          <select :id="'binding-' + key" v-model="draft[key]" :disabled="!editable"><option v-for="action in info?.actions || []" :key="action" :value="action">{{ info?.customActions?.find(a => a.id === action)?.label || actionLabels[action] || action }}{{ action === 'getup' ? '（长按）' : '' }}</option></select>
        </div>
        <div class="settings-actions">
          <button :disabled="!editable || !dirty" @click="save">保存映射</button>
          <button :disabled="!editable" @click="draft = { ...info!.defaults }">恢复默认到草稿</button>
          <button :disabled="!online || busy" @click="load">重新读取</button>
        </div>
      </section>
      <section class="settings-card"><h2>固定控制</h2>
        <dl><dt>Back / Select</dt><dd>急停卸力</dd><dt>B</dt><dd>停止保持</dd><dt>Start</dt><dd>实体手柄长按 2 秒卸力并关机；网页预留</dd><dt>LT</dt><dd>按住加速 ×2</dd><dt>RT</dt><dd>嘎一下；实体手柄支持双击和长按</dd><dt>十字键 ↓</dt><dd>长按 1.5 秒切换自主</dd><dt>左 / 右摇杆</dt><dd>行走转向 / 头部控制</dd></dl>
        <h2>触发方式</h2><p>普通动作单次按下触发。倒地起身无论绑在哪个键，都需长按约 0.56 秒。实体手柄换配置后需松开全部按键并回中。</p>
        <p>在「模型管理」新增动作后，这里的按键下拉框会显示新动作。动作使用当前选中的模型；是否允许执行，仍由模型、姿态和控制权检查决定。不绑定的按键不触发动作。</p>
        <p>实体手柄：{{ state.gamepadFeedback?.connected ? state.gamepadFeedback.device || '已连接' : '未连接' }}。此页只显示按键状态，不发送试运行指令。</p>
      </section>
    </div>
    <p v-if="message" role="status" :class="{ error }">{{ message }}</p>
  </section>
</template>
<style scoped>
.pad-settings{max-width:1200px;margin:auto;padding:24px}h1{font-size:28px;margin:8px 0}h2{font-size:17px;margin:0 0 16px}h2 small{font-size:12px;font-weight:400;color:#967026}p{line-height:1.7;color:#5c7265}.settings-grid{display:grid;grid-template-columns:1.1fr 1fr;gap:20px;margin-top:24px;align-items:start}.settings-card{padding:24px;background:white;border:1px solid #dce5df;border-radius:12px;min-width:0}.binding-row{display:grid;grid-template-columns:120px minmax(0,1fr);align-items:center;gap:12px;padding:9px 0;border-bottom:1px solid #eff3ef}.binding-row.pressed{background:#e3f2e8}label i{font-style:normal;font-size:11px;margin-left:8px;color:#268051}select,button{font:inherit;padding:9px;border:1px solid #c9d8ce;border-radius:6px;background:white;color:inherit;min-width:0}.settings-actions{display:flex;gap:8px;flex-wrap:wrap;margin-top:20px}.settings-actions button:first-child{background:#2e7051;color:white}dl{display:grid;grid-template-columns:110px 1fr;gap:12px;margin-bottom:28px}dt{font-weight:600}dd{margin:0;line-height:1.5}.error{color:#b13e35;overflow-wrap:anywhere}@media(max-width:950px){.settings-grid{grid-template-columns:1fr}}@media(max-width:500px){.pad-settings,.settings-card{padding:16px}.binding-row{grid-template-columns:90px minmax(0,1fr)}}
</style>
