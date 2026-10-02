<script setup lang="ts">
import { computed, ref, reactive, watch, onMounted, onBeforeUnmount } from 'vue';
import { useTelemetry } from '../store';
import { POLICY_OPTIONS, SKILLS, idleMove, keyboardMove, joystickMove, editableTarget, type Move } from '../modelControls';
import ModelRuntime from './ModelRuntime.vue';
const state = useTelemetry();
const root = ref<HTMLElement>();
const mode = ref<'manual' | 'preview' | 'robot'>('manual');
const active = ref(false), tab = ref<'drive' | 'skills'>('drive');
const selected = ref<string>('alpha_stand.onnx');
const action = ref('模型站立');
const move = reactive<Move>(idleMove());
const head = ref(0), mouth = ref(0);
const keys = new Set<string>();
let pointer: number | null = null;
const policy = computed(() => POLICY_OPTIONS.find(p => p.file === selected.value));
const enabled = computed(() => mode.value === 'preview' && active.value);
const walking = computed(() => enabled.value && policy.value?.kind === 'walk');
const status = computed(() => mode.value === 'manual' ? '手动控制 · 右侧滑块可操作实物' : active.value ? '操作预览中 · 模型推理未接入' : '操作预览待开始');
function zero() { keys.clear(); Object.assign(move, idleMove()); pointer = null; }
function stop() { active.value = false; zero(); action.value = '已停止预览'; }
function setMode(next: 'manual' | 'preview' | 'robot') { stop(); mode.value = next; }
function start() { mode.value = 'preview'; active.value = true; zero(); action.value = policy.value?.kind === 'walk' ? '零命令站立' : policy.value?.name || '模型站立'; }
function choose(file: string, name: string) { selected.value = file; action.value = name; zero(); }
function keyboard(event: KeyboardEvent, pressed: boolean) {
  if (event.key === 'Escape' && enabled.value) { zero(); action.value = '移动归零'; event.preventDefault(); return; }
  if (!walking.value || editableTarget(event.target) || !['w','s','a','d','q','e'].includes(event.key.toLowerCase())) return;
  event.preventDefault(); const key = event.key.toLowerCase();
  if (pressed) keys.add(key); else keys.delete(key);
  Object.assign(move, keyboardMove(keys));
}
function pointerMove(event: PointerEvent) {
  if (pointer !== event.pointerId || !walking.value) return;
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  Object.assign(move, joystickMove((event.clientX - box.left - box.width/2)/(box.width*.35), (event.clientY - box.top - box.height/2)/(box.height*.35)));
}
function pointerDown(event: PointerEvent) {
  if (!walking.value) return;
  event.preventDefault(); keys.clear(); pointer = event.pointerId;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId); root.value?.focus(); pointerMove(event);
}
function turn(direction: number) { if (walking.value) { zero(); move.turn = direction; } }
function loseFocus(event: FocusEvent) { if (!root.value?.contains(event.relatedTarget as Node)) zero(); }
function visibility() { if (document.hidden) stop(); }
watch(selected, zero);
watch(() => [state.endpoint, state.joints?.bootId, state.paused, state.connection], stop);
onMounted(() => { window.addEventListener('blur', zero); document.addEventListener('visibilitychange', visibility); });
onBeforeUnmount(() => { stop(); window.removeEventListener('blur', zero); document.removeEventListener('visibilitychange', visibility); });
</script>

<template>
  <section ref="root" class="model-controls" tabindex="0" aria-label="模型控制操作预览" @keydown="keyboard($event, true)" @keyup="keyboard($event, false)" @focusout="loseFocus">
    <div class="control-heading"><div><span class="control-symbol">◇</span><strong>模型控制</strong><span class="version">官方 v5</span></div><span class="backend-tag">{{ mode === 'robot' ? 'HD1910 v5 · 实机闭环' : '操作预览' }}</span></div>
    <div class="control-modes" role="group" aria-label="控制模式">
      <button :class="{ selected: mode === 'manual' }" :aria-pressed="mode === 'manual'" @click="setMode('manual')">手动</button>
      <button :class="{ selected: mode === 'preview' }" :aria-pressed="mode === 'preview'" @click="setMode('preview')">操作预览</button>
      <button :class="{ selected: mode === 'robot' }" @click="setMode('robot')">实机模型 <small>v5</small></button>
    </div>
    <ModelRuntime v-if="mode === 'robot'" />
    <template v-else>
    <div class="control-policy-row">
      <label><span>策略</span><select v-model="selected" aria-label="模型策略"><option v-for="item in POLICY_OPTIONS" :key="item.file" :value="item.file">{{ item.name }}</option></select></label>
      <button class="primary-control" :disabled="active" @click="start">▷ 开始预览</button>
      <button :disabled="!active" @click="stop">□ 停止</button>
    </div>
    <div class="control-tabs" role="group" aria-label="控制分区"><button :class="{ active: tab === 'drive' }" @click="tab='drive'">日常控制</button><button :class="{ active: tab === 'skills' }" @click="tab='skills'">动作技能</button><span>{{ active ? action : policy?.hint }}</span></div>
    <div v-if="tab === 'drive'" class="drive-content">
      <div class="joystick-section"><span class="joystick-caption">移动方向 <small>W / A / S / D</small></span>
        <div class="joystick" :class="{ inactive: !walking }" role="group" aria-label="移动摇杆；W/S前后，A/D横移，Q/E转向" @pointerdown="pointerDown" @pointermove="pointerMove" @pointerup="zero" @pointercancel="zero" @lostpointercapture="zero">
          <span class="north">前</span><span class="south">后</span><span class="west">左</span><span class="east">右</span>
          <i class="stick" :style="{ transform: `translate(calc(var(--stick-travel) * ${ -move.lateral }), calc(var(--stick-travel) * ${ -move.forward }))` }" />
        </div>
        <div class="turn-buttons"><button :disabled="!walking" @pointerdown="turn(1)" @pointerup="zero" @pointerleave="zero" @pointercancel="zero" @keydown.enter="turn(1)" @keyup.enter="zero" @keydown.space.prevent="turn(1)" @keyup.space="zero">↶ 左转</button><button :disabled="!enabled" @click="zero">归零</button><button :disabled="!walking" @pointerdown="turn(-1)" @pointerup="zero" @pointerleave="zero" @pointercancel="zero" @keydown.enter="turn(-1)" @keyup.enter="zero" @keydown.space.prevent="turn(-1)" @keyup.space="zero">右转 ↷</button></div>
      </div>
      <div class="daily-controls">
        <div class="pose-buttons"><button :disabled="!enabled" @click="choose('alpha_stand.onnx', '模型站立')">模型站立</button><button :disabled="!enabled" @click="choose('alpha_sitstand.onnx', '坐下')">坐下</button><button :disabled="!enabled" @click="choose('alpha_sitstand.onnx', '起立')">起立</button></div>
        <label class="preview-slider"><span>头部朝向</span><input v-model.number="head" type="range" min="-170" max="170" step="1" :disabled="!enabled" /><output>{{ head }}°</output></label>
        <label class="preview-slider"><span>嘴部开合</span><input v-model.number="mouth" type="range" min="0" max="30" step=".1" :disabled="!enabled" /><output>{{ mouth.toFixed(1) }}°</output></label>
        <div class="reset-buttons"><button :disabled="!enabled" @click="head=0; action='头部回正'">头部回正</button><button :disabled="!enabled" @click="zero(); action='身体回正'">身体回正</button></div>
        <p class="movement-readout">前后 {{ move.forward.toFixed(2) }} · 横移 {{ move.lateral.toFixed(2) }} · 转向 {{ move.turn.toFixed(2) }}</p>
      </div>
    </div>
    <div v-else class="skills-content"><button v-for="skill in SKILLS" :key="skill.file" :disabled="!enabled" @click="action=skill.name; zero()"><span>{{ skill.icon }}</span><strong>{{ skill.name }}</strong><small>{{ skill.hint }}</small></button><p>轮式行驶 / 下蹲：当前双足配置未启用</p></div>
    <footer class="control-footer" role="status"><span class="status-dot" :class="{ running: active }" />{{ status }}<small>{{ walking ? 'WASD / QE · 松手归零' : '预览仅检查操作，不运行模型或驱动舵机' }}</small></footer>
    </template>
  </section>
</template>

<style scoped>
.model-controls{position:relative;--disc-size:150px;--stick-travel:52.5px;flex-shrink:0;border-top:1px solid #dae4de;padding:10px 14px 8px;background:linear-gradient(135deg,#f8fbf9,#edf3ef);color:#345345;outline:none;display:grid;grid-template-columns:1fr auto;gap:7px 10px}
.model-controls:focus-visible{box-shadow:inset 0 0 0 2px #86aa92}
.control-heading,.control-heading>div{display:flex;align-items:center;gap:6px}.control-heading{flex-wrap:wrap;gap:0;height:30px;align-content:center}.control-heading strong{font-size:13px}.control-symbol{color:#4b8962;font-size:19px}.version{font-size:9px;color:#8a9c90}.backend-tag{font-size:9px;color:#89968d;width:100%;padding-left:24px}
button,select{font:inherit;color:inherit}button{border:1px solid #d4e0d8;background:#fff;border-radius:7px;padding:6px 9px;font-size:11px;cursor:pointer;transition:background .15s,box-shadow .15s}button:hover:not(:disabled){border-color:#80a388;background:#edf5ec;box-shadow:0 2px 6px #2543330e}button:disabled{opacity:.48;cursor:default}button:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid #78a28c;outline-offset:1px}
.control-modes{display:flex;align-self:center;padding:3px;background:#e3ebe5;border-radius:9px;gap:2px}.control-modes button{border:0;background:transparent;padding:5px 8px;font-size:10px}.control-modes button.selected{background:#fff;box-shadow:0 1px 4px #223f2417;color:#376749;font-weight:600}.control-modes small{font-size:8px;margin-left:3px}
.control-policy-row{grid-column:1/-1;display:flex;gap:6px;align-items:center}.control-policy-row label{display:flex;align-items:center;gap:7px;flex:1;min-width:0}.control-policy-row label>span{white-space:nowrap;font-size:10px;color:#7b9183}.control-policy-row select{width:100%;min-width:0;border:1px solid #d4dfd5;border-radius:7px;background:white;padding:6px;font-size:11px}.primary-control{background:#426e52;color:white;border-color:#426e52;white-space:nowrap}.primary-control:hover:not(:disabled){background:#365e45;color:white}.control-policy-row>button{white-space:nowrap}
.control-tabs{position:absolute;top:85px;left:200px;right:14px;display:flex;align-items:center;gap:15px;border-bottom:1px solid #dbe5de}.control-tabs button{border:0;background:transparent;border-radius:0;padding:2px 0 6px;color:#879a8d;font-size:11px}.control-tabs button.active{color:#345e45;border-bottom:2px solid #50765b;font-weight:600}.control-tabs span{display:none;margin-left:auto;font-size:9px;color:#8b9b90;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:50%}
.drive-content{grid-column:1/-1;display:grid;grid-template-columns:170px minmax(0,1fr);gap:16px;height:200px}.joystick-section{display:flex;flex-direction:column;align-items:center;justify-content:space-between}.joystick-caption{width:100%;font-size:10px;color:#5e7968;display:flex;justify-content:space-between;align-items:center}.joystick-caption small{font-size:8px;letter-spacing:1px;color:#91a398}
.joystick{width:var(--disc-size);height:var(--disc-size);flex-shrink:0;position:relative;border-radius:50%;border:1px solid #c3d4c8;background:radial-gradient(circle,#e8f0eb 0 29%,#c9d8ce 29.5% 30%,#f6faf7 30.5% 54%,#d5e1d9 54.5% 55%,#edf4ef 55.5% 68%,#dce7df 69%);box-shadow:inset 0 2px 8px #3454400e,0 4px 10px #3454400a;touch-action:none;cursor:grab;user-select:none}.joystick:active:not(.inactive){cursor:grabbing}.joystick.inactive{filter:saturate(.45);cursor:default}.joystick>span{position:absolute;font-size:11px;font-weight:600;color:#809888;pointer-events:none}.north{top:9px;left:50%;transform:translateX(-50%)}.south{bottom:9px;left:50%;transform:translateX(-50%)}.west{left:11px;top:50%;transform:translateY(-50%)}.east{right:11px;top:50%;transform:translateY(-50%)}.stick{position:absolute;width:42px;height:42px;left:calc(50% - 21px);top:calc(50% - 21px);border-radius:50%;background:radial-gradient(circle at 35% 25%,#7ea38c,#456f56 75%);box-shadow:0 4px 9px #31573c35,inset 0 1px 2px #ffffff80;border:3px solid #f7fbf8;box-sizing:border-box;pointer-events:none}.turn-buttons{display:flex;gap:4px;width:100%}.turn-buttons button{flex:1;font-size:10px;padding:4px 3px}
.daily-controls{display:flex;flex-direction:column;justify-content:space-between;padding:31px 0 0;gap:7px}.pose-buttons,.reset-buttons{display:flex;gap:5px}.pose-buttons button{flex:1;padding:8px 3px;font-size:11px}.reset-buttons button{flex:1;padding:5px;font-size:10px}.preview-slider{display:grid;grid-template-columns:1fr auto;gap:5px;font-size:11px}.preview-slider span{color:#6d8577}.preview-slider input{grid-column:1/-1;grid-row:2;width:100%;min-width:0;accent-color:#598668;height:17px;margin:0}.preview-slider output{text-align:right;font-size:11px;font-variant-numeric:tabular-nums}.movement-readout{margin:0;font-size:9px;color:#7d9485;font-variant-numeric:tabular-nums;white-space:nowrap}
.skills-content{grid-column:1/-1;height:200px;padding-top:27px;box-sizing:border-box;display:grid;grid-template-columns:repeat(2,1fr);gap:7px;align-content:start}.skills-content button{height:70px;display:grid;grid-template-columns:32px 1fr;align-content:center;text-align:left;gap:5px 8px;padding:10px}.skills-content button>span{grid-row:1/3;font-size:25px;color:#5d8761}.skills-content strong{font-size:12px;font-weight:600}.skills-content small{font-size:10px;color:#93a087}.skills-content p{grid-column:1/-1;font-size:9px;color:#94a28b;margin:3px 0}
.control-footer{margin:0;grid-column:1/-1;height:25px;border-top:1px solid #dbe5de;padding-top:5px;box-sizing:border-box;font-size:10px;color:#6d8576;display:flex;align-items:center;gap:5px;white-space:nowrap;overflow:hidden}.control-footer small{font-size:8px;color:#98a89e;margin-left:auto;overflow:hidden;text-overflow:ellipsis}.status-dot{display:inline-block;flex-shrink:0;width:6px;height:6px;border-radius:50%;background:#aab89e}.status-dot.running{background:#508366;box-shadow:0 0 0 3px #50836612}
@media(max-height:820px){.model-controls{position:relative;--disc-size:136px;--stick-travel:47.6px;padding:7px 10px 6px;gap:5px 7px}.drive-content,.skills-content{height:179px}.drive-content{grid-template-columns:152px minmax(0,1fr);gap:12px}.daily-controls{gap:5px;padding:27px 0 0}.control-tabs{top:76px;left:174px;right:10px}.pose-buttons button{padding:5px 3px}.control-footer{height:22px}.skills-content button{height:62px}}
@media(max-width:1100px){.model-controls{grid-template-columns:1fr;padding-left:8px;padding-right:8px}.control-modes{justify-content:center}.drive-content{grid-template-columns:145px minmax(0,1fr);gap:8px}.control-policy-row label>span,.version{display:none}.pose-buttons button{font-size:10px}.control-footer small{display:none}}
</style>
