<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { modelSlots, modelUpload, registryIdle, registryRequest, slotModels, type ModelRegistry } from '../modelRegistry';
const props = defineProps<{ endpoint: string; online: boolean; controlState?: string }>();
const emit = defineEmits<{ changed: []; busy: [value: boolean] }>();
const registry = ref<ModelRegistry>();
const choices = ref<Record<string, string>>({});
const loading = ref(false), working = ref(false), message = ref(''), failed = ref(false);
const slot = ref('walk'), label = ref('');
const onnx = ref<File>(), metadata = ref<File>();
const mode = ref('preset'), presetId = ref('');
const newActionId = ref(''), newActionLabel = ref(''), templateSlot = ref('dance');
const creating = computed(() => slot.value === '__new');
const slots = computed(() => [...modelSlots, ...(registry.value?.actions ?? [])]);
const executionSlot = computed(() => creating.value ? templateSlot.value : registry.value?.actions?.find(a => a.id === slot.value)?.templateSlot ?? slot.value);
const actionReady = computed(() => !creating.value || (/^[a-zA-Z0-9][a-zA-Z0-9_-]{0,55}$/.test(newActionId.value) && !!newActionLabel.value.trim()));
const customTemplates = modelSlots.filter(s => ['dance','pick','roulade','kick_left','kick_right'].includes(s.id));
const presets = computed(() => registry.value?.presets?.filter(p => p.slot === executionSlot.value) ?? []);
const preset = computed(() => presets.value.find(p => p.id === presetId.value));
watch(presets, values => { if (!values.some(p => p.id === presetId.value)) presetId.value = values[0]?.id ?? ''; });
const ready = computed(() => actionReady.value && !!onnx.value && !!label.value.trim() && (mode.value === 'preset' ? !!preset.value : !!metadata.value));
const canChange = computed(() => props.online && registryIdle(props.controlState) && !loading.value && !working.value && !!registry.value);
let generation = 0;
async function refresh() {
  const token = ++generation, endpoint = props.endpoint;
  registry.value = undefined; choices.value = {};
  if (!props.online) return;
  loading.value = true;
  try {
    const data: ModelRegistry = await registryRequest(endpoint);
    if (token !== generation) return;
    registry.value = data; choices.value = { ...data.selected };
    emit('changed');
  } catch (error) {
    if (token === generation) { failed.value = true; message.value = String(error); }
  } finally { if (token === generation) loading.value = false; }
}
watch(() => [props.endpoint, props.online], () => { message.value = ''; void refresh(); }, { immediate: true });
onUnmounted(() => { generation++; });
async function change(action: 'select' | 'rollback' | 'import', selectedSlot: string) {
  if (!canChange.value || !registry.value) return;
  const endpoint = props.endpoint, revision = registry.value.revision, token = generation;
  const id = choices.value[selectedSlot], name = label.value;
  working.value = true; emit('busy', true); failed.value = false; message.value = action === 'import' ? '正在上传并验证模型…' : '正在校验并预热所选模型…';
  try {
    let files = {};
    if (action === 'import') {
      if (!ready.value || !onnx.value) throw new Error('请填写名称、选择ONNX和对应配置');
      const upload = await modelUpload(onnx.value, mode.value === 'preset' ? undefined : metadata.value);
      files = mode.value === 'manifest'
        ? { onnxBase64: upload.onnxBase64, manifest: upload.metadata, onnxFilename: onnx.value.name }
        : { ...upload, ...(mode.value === 'preset' ? { presetId: presetId.value } : {}) };
    }
    if (token !== generation || endpoint !== props.endpoint) return;
    const targetSlot = creating.value && action === 'import' ? 'custom-' + newActionId.value : selectedSlot;
    const newAction = creating.value && action === 'import' ? { label: newActionLabel.value.trim(), templateSlot: templateSlot.value } : undefined;
    await registryRequest(endpoint, action, { revision, slot: targetSlot, id, label: name.trim(), ...files, ...(newAction ? { newAction } : {}) });
    if (newAction) slot.value = targetSlot;
    if (token !== generation) return;
    message.value = action === 'import' ? '模型已导入。新动作已配置首个模型，可在手柄设置中绑定；已有动作的候选模型请在左侧应用。' : '选择已保存，下次启动模型时生效。';
  } catch (error) {
    if (token === generation) { failed.value = true; message.value = String(error); }
  } finally {
    working.value = false; emit('busy', false);
    if (token === generation) await refresh();
  }
}
function file(event: Event, target: 'onnx' | 'metadata') {
  const chosen = (event.target as HTMLInputElement).files?.[0];
  if (target === 'onnx') onnx.value = chosen; else metadata.value = chosen;
}
</script>
<template>
  <section class="model-registry">
    <div class="eyebrow">POLICY LIBRARY</div>
    <h1>模型管理</h1>
    <p>各动作使用下方指定的模型。停止控制后可更换，导入和切换不会启动机器人。</p>
    <button :disabled="!online || working || loading" @click="refresh">{{ loading ? '读取中…' : '刷新模型列表' }}</button>
    <p v-if="online && !registryIdle(controlState)">请先停止模型及其他控制任务，再更换模型。</p>
    <div class="manager-columns">
    <section class="model-card">
    <h2>动作模型</h2><button :disabled="!canChange" @click="slot = '__new'">＋ 新增动作</button>
    <p v-if="!online">服务离线，连接后可读取模型。</p>
    <div v-if="registry" class="model-slots">
      <div v-for="item in slots" :key="item.id" class="model-slot">
        <label :for="'model-slot-' + item.id">{{ item.label }}</label>
        <select :id="'model-slot-' + item.id" v-model="choices[item.id]" :disabled="!canChange">
          <option v-for="model in slotModels(registry, item.id)" :key="model.id" :value="model.id" :disabled="!model.available">
            {{ model.label }}{{ !model.available ? '（不可用）' : '' }}{{ registry.selected[item.id] === model.id ? ' · 当前' : '' }}
          </option>
        </select>
        <button :disabled="!canChange || choices[item.id] === registry.selected[item.id] || !registry.models.find(m => m.id === choices[item.id])?.available" @click="change('select', item.id)">应用</button>
        <button :disabled="!canChange || !registry.previous[item.id]" @click="change('rollback', item.id)">上一版本</button>
        <template v-for="model in slotModels(registry, item.id).filter(m => m.id === choices[item.id])" :key="model.id">
          <small class="model-detail">{{ model.error || `${model.metadata?.runtimeKind || '—'} · SHA ${model.metadata?.policySha256?.slice(0, 12) || '—'}` }}<span v-if="model.metadata?.hardwareValidated === false"> · 尚未完成实机验收</span></small>
        </template>
      </div>
    </div>
    </section>
    <fieldset :disabled="!canChange" class="model-import model-card">
      <legend>{{ creating ? '新增动作与模型' : '导入模型' }}</legend>
      <label>动作 <select v-model="slot"><option value="__new">＋ 新增自定义动作</option><option v-for="item in slots" :key="item.id" :value="item.id">{{ item.label }}</option></select></label>
      <template v-if="creating">
        <label>动作名称 <input v-model="newActionLabel" maxlength="80" placeholder="例如：招手" /></label>
        <label>动作 ID <input v-model="newActionId" maxlength="56" placeholder="例如：wave（字母、数字、下划线或短横线）" /></label>
        <label>执行方式 <select v-model="templateSlot"><option v-for="item in customTemplates" :key="item.id" :value="item.id">{{ item.label }}兼容模板</option></select></label>
        <p>新动作拥有独立模型和手柄绑定。选择与训练一致的执行方式；动作的观测、命令和结束时序沿用该模板，完成后返回平衡。上传成功后即可在「手柄设置」选择绑定。</p>
      </template>
      <label>模型名称 <input v-model="label" maxlength="80" placeholder="例如：行走候选 6000" /></label>
      <label>ONNX <input type="file" accept=".onnx" @change="file($event, 'onnx')" /></label>
      <label>配置来源 <select v-model="mode"><option value="preset">默认执行模板</option><option value="manifest">官方 manifest.json</option><option value="metadata">完整 metadata.json</option></select></label>
      <template v-if="mode === 'preset'">
        <label>执行模板 <select v-model="presetId"><option v-for="item in presets" :key="item.id" :value="item.id">{{ item.label }}</option></select></label>
        <p>适用于与模板使用相同观测、关节顺序、动作含义和时序的自训模型。服务自动计算新权重的SHA；训练来源和验收结论不继承。</p>
        <details v-if="preset"><summary>查看默认配置</summary><pre>{{ JSON.stringify(preset.metadata, null, 2) }}</pre></details>
      </template>
      <template v-else>
        <label>{{ mode === 'manifest' ? 'Manifest' : 'Metadata' }} <input type="file" accept=".json" @change="file($event, 'metadata')" /></label>
        <p v-if="mode === 'manifest'">支持官方schema 2的单模型和模型集（按ONNX文件名匹配）。当前适配BallKick左右脚（0.5秒）和FlamingoCycleHard（左脚抬起）；仅限前馈API 1。XL330模型在HD1910上仍需实机验证。</p>
      </template>
      <button :disabled="!ready" @click="change('import', slot)">{{ creating ? '创建动作并验证模型' : '上传并验证' }}</button>
      <small>ONNX上限16MiB，JSON上限256KiB。验证通过后加入候选列表，请在左侧应用。</small>
    </fieldset>
    </div>
    <p v-if="message" :class="{ error: failed }" role="status" aria-live="polite">{{ message }}</p>
  </section>
</template>
<style scoped>
.model-registry { max-width: 1400px; margin: 0 auto; padding: 24px; }
h1 { margin: 8px 0; font-size: 28px; }
h2 { font-size: 17px; margin: 0 0 12px; }
.manager-columns { display: grid; grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr); gap: 20px; margin-top: 20px; align-items: start; }
.model-card { background: #fff; border: 1px solid #dce5df; border-radius: 12px; padding: 20px; min-width: 0; }
button, select, input { padding: 8px; border: 1px solid #c9d8ce; border-radius: 6px; background: #fff; color: inherit; }
pre { max-height: 280px; overflow: auto; font-size: 11px; background: #f5f7f4; padding: 10px; }
legend { font-size: 17px; font-weight: 600; }
summary { cursor: pointer; font-weight: 600; }
p, small { font-size: 12px; line-height: 1.5; }
.model-slots { display: grid; gap: 10px; margin-top: 12px; }
.model-slot { display: grid; grid-template-columns: 74px minmax(0, 1fr) auto auto; gap: 5px; align-items: center; }
select, input { min-width: 0; max-width: 100%; }
.model-detail { grid-column: 1 / -1; overflow-wrap: anywhere; opacity: .8; }
.model-import { display: grid; gap: 8px; margin: 0; }
.model-import label { display: grid; grid-template-columns: 70px minmax(0, 1fr); gap: 8px; align-items: center; }
.error { color: #d64b4b; overflow-wrap: anywhere; }
@media (max-width: 1050px) { .manager-columns { grid-template-columns: 1fr; } }
@media (max-width: 540px) { .model-slot { grid-template-columns: minmax(0, 1fr) auto auto; } .model-slot > label { grid-column: 1 / -1; } }
</style>
