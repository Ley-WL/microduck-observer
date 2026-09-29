<script setup lang="ts">
import { computed, ref } from 'vue';
import { useTelemetry } from '../store';
import RobotView from './RobotView.vue';
import CameraView from './CameraView.vue';
import TofRadar from './TofRadar.vue';
import TofDepthMap from './TofDepthMap.vue';
import SignalChart from './SignalChart.vue';
import { tofStats } from '../tof';
const props = defineProps<{ quaternion: number[] | null; angles: number[] | null; calibrated: boolean }>();
const emit = defineEmits<{ navigate: [page: '姿态与 IMU'] }>();
const state = useTelemetry();
const depthMode = ref('radar');
const robot = ref<InstanceType<typeof RobotView>>();
const selected = ref(27), range = ref(2000), smooth = ref(true);
const chartKind = ref<'gyro' | 'accel'>('gyro');
const imuLive = computed(() => state.connection === '在线' && state.imuState === '实时' && state.poseState === '实时');
const tofLive = computed(() => state.connection === '在线' && state.tof?.valid && state.tofAge < 1500 && state.system?.data.tof?.state !== 'offline');
const cells = computed(() => Array.from({length:64}, (_, i) => ({distance:state.tof?.data.distanceMm[i],valid:state.tof?.data.status[i] === 5})));
const stats = computed(() => tofStats(state.tof?.data as any));
const accel = computed(() => state.imu?.valid ? Math.hypot(...state.imu.data.accel) : null);
const gyro = computed(() => state.imu?.valid ? Math.hypot(...state.imu.data.gyro) : null);
const steady = computed(() => imuLive.value && accel.value !== null && gyro.value !== null && accel.value > 8.3 && accel.value < 11.3 && gyro.value <= .08);
const fmt = (n: number | null | undefined, digits=1) => n == null || !Number.isFinite(n) ? '—' : n.toFixed(digits);
const age = (n: number) => Number.isFinite(n) ? `${Math.round(n)} ms` : '—';
const distance = (n: number | null | undefined) => n == null ? '—' : `${(n/10).toFixed(1)} cm`;
</script>

<template>
  <section class="sensor-workbench">
    <div class="workbench-heading">
      <div><h1>传感器联合观测 <small>SENSOR WORKSPACE</small></h1><p>姿态、视觉与距离 · 同屏实时调试</p></div>
      <div class="heading-actions"><span>{{ state.connection }} · {{ state.source }}</span><button @click="state.connect()">重连遥测</button></div>
    </div>
    <div v-if="state.error || state.paused" class="workbench-alert">{{ state.error || '遥测显示已暂停，摄像头仍实时播放。' }}<button v-if="state.paused" @click="state.paused = false">恢复遥测</button></div>
    <div class="sensor-columns">
      <section class="sensor-card imu-card">
        <div class="sensor-title-row"><div class="sensor-identity"><span class="sensor-icon">⌁</span><div><h2>IMU 姿态</h2><small>{{ state.info?.imuModel || 'IMU' }} · ORIENTATION</small></div></div><span class="live-badge" :class="{ live: imuLive }">{{ imuLive ? '实时' : '离线 / 过期' }}</span></div>
        <div class="stream-meta"><span>{{ state.info?.imuModel || 'IMU' }} · {{ fmt(state.hz) }} Hz</span><span>样本年龄 {{ age(state.imuAge) }}</span></div>
        <div class="imu-toolbar"><span>3D 姿态 · 拖动旋转视角</span><button @click="robot?.home()">复位视角 ↺</button></div>
        <div class="model-stage" :class="{ stale: !imuLive }"><RobotView ref="robot" :quaternion="quaternion" :paused="state.paused || !imuLive" /><span class="model-caption">{{ calibrated ? '已应用姿态参考' : '传感器原始朝向 · 未标定' }}</span></div>
        <div class="angle-readings"><div v-for="(name,i) in ['横滚 Roll','俯仰 Pitch','航向 Yaw']" :key="name"><span>{{ name }}</span><strong>{{ fmt(angles?.[i]) }}<small>°</small></strong></div></div>
        <div class="motion-state" :class="{ ready: steady }">● {{ !imuLive ? '等待新鲜 IMU 数据' : steady ? '当前静止' : '运动中 / 重力读数需检查' }}<small>仅即时读数，标定还需持续静止</small></div>
        <div class="raw-readings"><span>加速度模长 <b>{{ fmt(accel,2) }} m/s²</b></span><span>角速度模长 <b>{{ fmt(gyro,3) }} rad/s</b></span></div>
        <div class="chart-title"><span>最近 60 秒 · X / Y / Z</span><select v-model="chartKind"><option value="gyro">角速度 rad/s</option><option value="accel">加速度 m/s²</option></select></div>
        <div class="mini-chart"><SignalChart :samples="state.chart" :kind="chartKind" /></div>
        <button class="detail-link" @click="emit('navigate','姿态与 IMU')">IMU 详情与标定 →</button>
      </section>

      <CameraView compact />

      <section class="sensor-card depth-card">
        <div class="sensor-title-row"><div class="sensor-identity"><span class="sensor-icon">◉</span><div><h2>ToF 深度</h2><small>VL53L5CX · DEPTH</small></div></div><span class="live-badge" :class="{live:tofLive}">{{ tofLive ? '实时' : '离线 / 过期' }}</span></div>
        <div class="stream-meta"><span>ToF · {{ tofLive ? fmt(state.tof?.data.hz) : '—' }} Hz</span><span>样本年龄 {{ age(state.tofAge) }}</span></div>
        <div class="depth-controls"><select v-model="depthMode" aria-label="ToF显示方式"><option value="radar">扇环距离</option><option value="depth">灰度深度</option></select><label v-if="depthMode === 'depth'"><input v-model="smooth" type="checkbox" /> 平滑</label><select v-model.number="range"><option :value="1000">0–100 cm</option><option :value="2000">0–200 cm</option><option :value="4000">0–400 cm</option></select></div>
        <div class="depth-stage" :class="{ stale: !tofLive || state.paused }"><TofRadar v-if="depthMode === 'radar'" :cells="cells" :range="range" :selected="selected" :live="Boolean(tofLive) && !state.paused" @select="selected=$event" /><TofDepthMap v-else :cells="cells" :range="range" :smooth="smooth" :selected="selected" @select="selected=$event" /></div>
        <div v-if="depthMode === 'depth'" class="depth-legend"><span>近 0</span><i></i><span>{{ range / 10 }} cm 远</span></div>
        <div class="selected-depth"><span>选点 · {{ Math.floor(selected/8)+1 }}行 {{ selected%8+1 }}列</span><strong>{{ cells[selected].valid ? distance(cells[selected].distance) : '无效测量' }}</strong><small>状态 {{ state.tof?.data.status[selected] ?? '—' }} · 原始分区读数</small></div>
        <div class="depth-stats"><span>最近距离<b>{{ distance(stats.min) }}</b></span><span>中位距离<b>{{ distance(stats.median) }}</b></span><span>有效区域<b>{{ stats.count }} / 64</b></span></div>
        <p v-if="!tofLive" class="depth-warning">{{ state.tof ? '当前为最后一帧，不代表实时距离。' : '等待 ToF 数据。' }} {{ state.system?.data.tof?.error }}</p>
        <p class="depth-help">扇环按8列取最近有效回波，金色圈标记最近点；点击查看原始距离。</p>
      </section>
    </div>
    <div class="workbench-footnote">视频与遥测独立实时接收，未做时间同步或视场配准；样本年龄不等于端到端延迟。ToF 图不叠加在视频上。</div>
  </section>
</template>

<style scoped>
.sensor-workbench{height:100%;min-height:0;display:flex;flex-direction:column;gap:10px;color:#233f34}
.workbench-heading{display:flex;justify-content:space-between;align-items:center;gap:12px;flex-shrink:0}.workbench-heading h1{font-size:23px;margin:2px 0 5px}.workbench-heading h1 small{font-size:10px;font-weight:500;letter-spacing:2px;margin-left:12px;color:#74877c}.workbench-heading p{margin:0;font-size:12px;color:#718176}
.heading-actions{display:flex;gap:12px;align-items:center;font-size:12px}.sensor-workbench button,.sensor-workbench select{font:inherit;border:1px solid #d9e3dd;border-radius:6px;background:#fff;color:#365347;padding:6px 9px;cursor:pointer}.sensor-workbench select{font-size:11px;padding:4px}
.sensor-columns{display:grid;grid-template-columns:minmax(250px,3fr) minmax(320px,4fr) minmax(250px,3fr);gap:12px;flex:1;min-height:0}
.sensor-card{background:white;border:1px solid #dfe7e2;border-radius:14px;padding:14px;display:flex;flex-direction:column;gap:10px;min-height:0;overflow:hidden}
.card-heading{display:flex;align-items:center;justify-content:space-between;gap:8px}.card-heading h2{font-size:18px;margin:0}.live-badge{font-size:11px;border-radius:20px;padding:5px 9px;background:#fff0db;color:#9a5815}.live-badge.live{background:#e1f3e7;color:#267448}
.stream-meta{font-size:10px;display:flex;justify-content:space-between;color:#718176;font-variant-numeric:tabular-nums}
.model-stage{position:relative;flex:1;min-height:120px;border-radius:10px;background:radial-gradient(ellipse,#f1f8f4,#e7efe9);overflow:hidden}.model-stage :deep(.robot-canvas){position:absolute;inset:0;height:100%;min-height:0;width:100%}.model-stage .home-view{position:absolute;top:8px;right:8px;font-size:10px}.model-caption{position:absolute;bottom:8px;left:10px;font-size:10px;color:#6c7f73;pointer-events:none}
.angle-readings{display:grid;grid-template-columns:repeat(3,1fr);gap:8px}.angle-readings>div{background:#f3f7f4;padding:9px 6px;border-radius:8px}.angle-readings span{display:block;font-size:10px;color:#738478}.angle-readings strong{display:block;font-size:23px;margin-top:4px;font-variant-numeric:tabular-nums}.angle-readings small{font-size:13px;font-weight:400}
.motion-state{font-size:11px;color:#9a5815}.motion-state.ready{color:#267448}.motion-state small{display:block;color:#8a978f;font-size:10px;margin-top:3px}.raw-readings{display:flex;justify-content:space-between;gap:8px;font-size:10px;color:#748277}.raw-readings b{display:block;color:#355b47;margin-top:4px;font-variant-numeric:tabular-nums}
.chart-title{display:flex;align-items:center;justify-content:space-between;font-size:10px;color:#7b8980}.mini-chart{height:100px;flex-shrink:0}.mini-chart :deep(.signal-chart){height:100%}.mini-chart :deep(div){max-height:100%}.sensor-workbench .detail-link{font-size:11px;border:0;background:#f1f6f3;color:#397b57;padding:8px}
.depth-controls{display:flex;align-items:center;justify-content:space-between;gap:5px;font-size:10px;color:#748277}.depth-controls label{display:flex;align-items:center;gap:3px}.depth-stage{min-height:0;flex:1;display:flex;align-items:center;justify-content:center}.depth-stage :deep(.depth-image){width:100%;max-height:100%;aspect-ratio:1;max-width:calc(100dvh - 440px)}
.depth-legend{display:flex;align-items:center;gap:8px;font-size:10px;color:#748277}.depth-legend i{height:7px;background:linear-gradient(90deg,#f5f5f5,#141414);border:1px solid #dae2dc;flex:1;border-radius:3px}
.selected-depth{padding:12px;border-radius:10px;background:#f2f6f4}.selected-depth span,.selected-depth small{display:block;font-size:10px;color:#738478}.selected-depth strong{display:block;font-size:30px;margin:5px 0;font-variant-numeric:tabular-nums}.depth-stats{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;font-size:10px;color:#748277}.depth-stats b{display:block;font-size:13px;color:#355b47;margin-top:6px}.depth-help,.depth-warning{font-size:10px;line-height:1.6;margin:0;color:#7c897f}.depth-warning{color:#ac6527}.stale{opacity:.45}
.workbench-footnote{font-size:10px;color:#829086;flex-shrink:0}.workbench-alert{background:#fff1dc;color:#915822;padding:6px 10px;font-size:12px;border-radius:6px}.workbench-alert button{margin-left:10px}
@media(max-height:760px){.sensor-card{gap:7px;padding:11px}.mini-chart{height:65px}.workbench-heading p{display:none}.selected-depth{padding:8px}.selected-depth strong{font-size:23px}.angle-readings strong{font-size:20px}.depth-stage :deep(.depth-image){max-width:calc(100dvh - 380px)}}
@media(max-width:1000px){.sensor-columns{gap:8px;grid-template-columns:minmax(210px,3fr) minmax(245px,4fr) minmax(210px,3fr)}.sensor-card{padding:10px}.heading-actions span{display:none}.workbench-heading h1 small{display:none}}
@media(max-width:760px),(max-height:570px){.sensor-workbench{overflow:auto}.sensor-columns{flex:none;grid-template-columns:1fr}.sensor-card{min-height:570px}.sensor-columns :deep(.camera-page.compact){min-height:420px}.depth-stage :deep(.depth-image){max-width:300px}.workbench-footnote{line-height:1.6}}
/* Shared visual language across all three sensor panels, including camera. */
.sensor-workbench{gap:14px;color:#29483e;padding:4px 2px 0}.workbench-heading{padding:5px 4px 9px}.workbench-heading h1{font-size:22px;font-weight:650;letter-spacing:-.5px}.workbench-heading h1 small{letter-spacing:1.7px;font-size:9px;color:#92a297}.workbench-heading p{font-size:11px;color:#89988e}.heading-actions{font-size:11px;color:#819187}.heading-actions button{border-radius:8px;padding:8px 12px;box-shadow:0 2px 4px #173c2404}
.sensor-columns{gap:16px}.sensor-card{border:1px solid #e0e8e5;border-radius:18px;padding:16px;gap:12px;box-shadow:0 4px 22px #18372b05}
.sensor-workbench :deep(.sensor-title-row){display:flex;align-items:center;justify-content:space-between;gap:8px;min-height:42px;flex-shrink:0}
.sensor-workbench :deep(.sensor-identity){display:flex;align-items:center;gap:10px;min-width:0}.sensor-workbench :deep(.sensor-icon){display:grid;place-items:center;flex-shrink:0;width:38px;height:38px;background:#eef5f1;border:1px solid #e4eee8;border-radius:11px;color:#548874;font-size:23px}
.sensor-workbench :deep(.sensor-identity h2){font-size:17px;line-height:1.3;font-weight:650;margin:0;color:#29483e;white-space:nowrap}.sensor-workbench :deep(.sensor-identity small){display:block;margin-top:5px;font-size:9px;letter-spacing:1px;color:#92a297}
.sensor-workbench :deep(.live-badge){font-size:10px;font-weight:500;padding:5px 8px;white-space:nowrap;border-radius:6px;background:#fbf1e3;color:#aa7c38;flex-shrink:0}.sensor-workbench :deep(.live-badge.live){background:#eaf5ef;color:#358361}
.sensor-workbench :deep(.stream-meta){display:flex;justify-content:space-between;font-size:10px;color:#8b9b91;font-variant-numeric:tabular-nums;min-height:14px}
.imu-toolbar,.depth-controls{height:30px;min-height:30px;display:flex;align-items:center;justify-content:space-between;gap:6px;color:#8b9b91;font-size:10px}.imu-toolbar button{font-size:10px;border-color:#e0e7e6;background:#f7f9f8;padding:6px 8px}.depth-controls select{border-color:#e0e7e6;background:#f7f9f8}.depth-controls input{accent-color:#548874}
.model-stage{background:radial-gradient(ellipse at 50% 45%,#f8fbf9,#edf3ef);border:1px solid #edf2ef;border-radius:12px}.model-caption{background:#ffffffa6;border:1px solid #e7eee9;padding:4px 7px;border-radius:5px;color:#8a9d90;font-size:9px}
.angle-readings>div{border:1px solid #edf1ef;background:#f7f9f8;border-radius:9px;padding:10px 8px}.angle-readings strong{font-size:24px;font-weight:550;letter-spacing:-.8px}.angle-readings span{color:#8a9b90;font-size:9px}.angle-readings small{color:#99a79f;margin-left:2px}
.motion-state{padding:8px 10px;border:1px solid #f2e7d7;border-radius:8px;background:#fdf8f0}.motion-state.ready{background:#f3f9f5;border-color:#e4f0e8}.motion-state small{font-size:9px}.raw-readings{padding:0 3px}.raw-readings b{font-weight:550;font-size:12px}.sensor-workbench .detail-link{margin-top:auto;border:1px solid #e5ede8;background:#f6f9f7;border-radius:8px;color:#678675;padding:8px}
.depth-stage{background:#f6f8f7;border:1px solid #edf1ef;border-radius:12px;padding:10px}.depth-stage :deep(.depth-image){border-radius:8px;max-width:calc(100dvh - 480px)}.depth-legend{padding:0 4px;color:#92a196}.selected-depth{border:1px solid #e5eee8;background:linear-gradient(120deg,#f2f7f4,#f8faf9);padding:13px}.selected-depth strong{font-weight:550;letter-spacing:-.6px;font-size:29px;color:#315c48}.selected-depth small{font-size:9px}.depth-stats{padding:4px 2px}.depth-stats b{font-size:14px;font-weight:550}.depth-stats>span+span{border-left:1px solid #e9eeeb;padding-left:10px}.depth-help{color:#93a095;font-size:9px}.workbench-footnote{font-size:9px;color:#99a59d;padding:0 4px}
@media(max-height:760px){.sensor-workbench{gap:8px}.workbench-heading{padding:0 4px 4px}.sensor-card{gap:8px;padding:12px}.sensor-columns{gap:12px}.angle-readings>div{padding:7px}.angle-readings strong{font-size:20px}.motion-state{padding:6px 8px}.selected-depth{padding:9px}.selected-depth strong{font-size:25px}.depth-stage :deep(.depth-image){max-width:calc(100dvh - 405px)}}
@media(max-width:1100px){.sensor-workbench :deep(.sensor-identity){gap:7px}.sensor-workbench :deep(.sensor-identity h2){font-size:14px}.sensor-workbench :deep(.sensor-icon){width:30px;height:32px;font-size:20px}.sensor-workbench :deep(.sensor-identity small){font-size:8px;letter-spacing:.4px}.sensor-columns{gap:10px}.sensor-card{padding:11px}}
@media(max-width:760px),(max-height:570px){.depth-stage :deep(.depth-image){max-width:300px}.sensor-workbench :deep(.sensor-identity h2){font-size:17px}}
</style>
