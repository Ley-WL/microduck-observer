<script setup lang="ts">
import { computed, ref, onMounted, onBeforeUnmount } from 'vue';
import { Euler, Quaternion } from 'three';
import RobotView from './RobotView.vue';
import { useBoardCalibration } from '../boardCalibration';
import { useTelemetry } from '../store';
import { sensorToTrunk, trunkToSensor } from '../imuInstallation';
import { bodyRelativeQuaternion } from '../calibration';
const emit = defineEmits<{ close: [] }>();
const dialog = ref<HTMLElement>();
function close() { emit('close'); }
function escape(event: KeyboardEvent) { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } }
onMounted(() => { dialog.value?.focus(); window.addEventListener('keydown', escape, true); });
onBeforeUnmount(() => window.removeEventListener('keydown', escape, true));
const board = useBoardCalibration(), state = useTelemetry();
const initial = JSON.parse(JSON.stringify(board.data));
const endpoint = state.endpoint;
const identity = [0,0,0,1], neutralJoints: Record<number,number> = {};
const draft = ref({ positionMm: [...(initial?.mounting?.positionMm ?? [0,0,0])], quaternion: sensorToTrunk(initial?.imu, initial?.mounting?.yaw ?? -90) });
const mode = ref<'translate'|'rotate'>('translate'), message = ref('');
const reference = initial?.imu?.quaternion;
const preview = computed(() => {
  if (!state.orientation?.valid || !reference) return null;
  return bodyRelativeQuaternion(reference, state.orientation.data.quaternion, trunkToSensor(draft.value.quaternion), initial.imu.targetQuaternion);
});
const rotation = computed(() => {
  const e = new Euler().setFromQuaternion(new Quaternion().fromArray(draft.value.quaternion),'XYZ');
  return [e.x,e.y,e.z].map(n=>n*180/Math.PI);
});
function position(axis:number,event:Event) {
  const n=Number((event.target as HTMLInputElement).value);
  if (Number.isFinite(n)) { const p=[...draft.value.positionMm];p[axis]=Math.max(-300,Math.min(300,n));draft.value={...draft.value,positionMm:p}; }
}
function turn(axis:number,degrees:number) {
  const v=[0,0,0];v[axis]=degrees*Math.PI/180;
  const q=new Quaternion().fromArray(draft.value.quaternion).multiply(new Quaternion().setFromEuler(new Euler(...v as [number,number,number],'XYZ')));
  draft.value={...draft.value,quaternion:q.normalize().toArray()};
}
const contextChanged = computed(() => endpoint!==state.endpoint || initial?.revision!==board.data?.revision);
async function save() {
  message.value='';
  await board.installation(draft.value.positionMm,trunkToSensor(draft.value.quaternion),initial.revision);
  if (board.error) message.value=board.error;
  else emit('close');
}
</script>
<template>
  <Teleport to="body">
  <div class="installation-shade" @click.self="close">
    <section ref="dialog" tabindex="-1" class="installation-dialog" role="dialog" aria-modal="true" aria-label="IMU 安装设置">
      <div class="installation-title"><div><h2>IMU 安装位置与方向</h2><p>固定在躯干 · X 向前 / Y 向左 / Z 向上 · 红 X、绿 Y、蓝 Z 为传感器轴</p></div><button type="button" aria-label="关闭安装设置" @click="close">关闭 ×</button></div>
      <div class="installation-layout">
        <div class="edit-model"><RobotView :quaternion="identity" :paused="false" :preview-angles="neutralJoints" :installation="draft" :edit-mode="mode" @installation-change="draft=$event" /><div class="mode-controls"><button :class="{active:mode==='translate'}" @click="mode='translate'">移动安装点</button><button :class="{active:mode==='rotate'}" @click="mode='rotate'">旋转安装方向</button></div></div>
        <div class="installation-settings">
          <h3>拖动青色 IMU 的坐标手柄</h3><p>箭头移动、圆环旋转；拖动背景可旋转模型。位置以躯干原点计，不自动吸附表面。</p>
          <div class="position-inputs"><label v-for="(axis,i) in ['X','Y','Z']" :key="axis">{{ axis }} mm<input type="number" min="-300" max="300" step="1" :value="Number(draft.positionMm[i].toFixed(1))" @change="position(i,$event)" /></label></div>
          <div v-for="(axis,i) in ['X','Y','Z']" :key="axis" class="rotation-row"><span>{{ axis }} {{ rotation[i].toFixed(1) }}°</span><button @click="turn(i,-90)">−90°</button><button @click="turn(i,90)">+90°</button></div>
          <h3>换算后姿态预览</h3><div class="installation-preview"><RobotView :quaternion="preview" :paused="!preview || state.poseState!=='实时'" :installation="draft" /></div>
          <p>{{ !reference ? '尚无姿态参考：保存安装方向后，请进行仰卧或站立标定。' : '右侧按草稿方向实时换算；左侧保持固定姿态方便编辑。' }}</p>
          <p>移动位置仅更新安装点；方向自动转换为安装矩阵，不会移动实体舵机。</p>
          <p v-if="contextChanged || message" class="installation-error">{{ message || '主板标定或连接已改变，请关闭后重新打开。' }}</p>
          <button class="save-installation" :disabled="board.saving || !board.ready || contextChanged || state.connection!=='在线'" @click="save">{{ board.saving ? '保存中…' : '保存安装设置到主控' }}</button>
        </div>
      </div>
    </section>
  </div>
  </Teleport>
</template>
<style scoped>
.installation-shade{position:fixed;inset:0;background:#112b236b;z-index:1000;display:grid;place-items:center;padding:24px}.installation-dialog{width:min(1150px,96vw);height:min(790px,94dvh);background:#f6f9f7;border-radius:18px;display:flex;flex-direction:column;padding:20px;box-shadow:0 20px 80px #10271e44;color:#29483e}.installation-title{display:flex;justify-content:space-between;gap:16px;align-items:center}.installation-title h2{margin:0;font-size:21px}.installation-dialog p{font-size:12px;line-height:1.6;color:#70867a}.installation-layout{display:grid;grid-template-columns:1fr 320px;gap:20px;flex:1;min-height:0}.edit-model{position:relative;min-height:0;background:#e8f0eb;border-radius:14px;overflow:hidden}.edit-model :deep(.robot-canvas),.installation-preview :deep(.robot-canvas){height:100%;min-height:0;width:100%}.mode-controls{position:absolute;top:12px;left:12px;display:flex;gap:8px}.installation-dialog button{padding:8px 12px;border:1px solid #cbded0;border-radius:7px;background:white;color:#36674e;cursor:pointer}.installation-dialog button.active,.installation-dialog .save-installation{background:#287454;color:white}.installation-dialog button:disabled{opacity:.4;cursor:default}.installation-settings{overflow:auto}.installation-settings h3{font-size:14px;margin:12px 0}.position-inputs{display:flex;gap:8px}.position-inputs label{font-size:11px;width:33%}.position-inputs input{width:100%;padding:7px;border:1px solid #ccdcd2;border-radius:5px;margin:5px 0}.rotation-row{display:flex;gap:8px;align-items:center;margin:6px 0;font-size:12px}.rotation-row span{flex:1}.installation-preview{position:relative;isolation:isolate;height:170px;background:#e8f0eb;border-radius:10px;overflow:hidden}.installation-settings .installation-error{color:#b44833}.save-installation{width:100%}@media(max-width:750px){.installation-layout{grid-template-columns:1fr;overflow:auto}.edit-model{min-height:320px}.installation-settings{overflow:visible}}

.installation-dialog{position:relative;isolation:isolate;outline:none}
.installation-title{position:relative;z-index:3;flex-shrink:0}
.edit-model,.installation-preview{position:relative;isolation:isolate;overflow:hidden}
.edit-model :deep(.robot-canvas),.installation-preview :deep(.robot-canvas){position:absolute;inset:0;z-index:0}
.mode-controls{z-index:2;pointer-events:auto}.installation-settings{position:relative;z-index:1}
</style>
