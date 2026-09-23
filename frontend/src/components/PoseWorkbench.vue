<script setup lang="ts">
import { ref, computed } from "vue";
import { useTelemetry } from "../store";
import RobotView from "./RobotView.vue";
import SignalChart from "./SignalChart.vue";
import ServoPanel from "./ServoPanel.vue";
import { useJointPose } from "../jointPose";
const jointPose = useJointPose();
const props = defineProps<{
  modelOrientation: number[] | null;
  quaternion: number[] | null;
  angles: number[] | null;
  calibrated: boolean;
  calibrationTime: string;
  canCalibrate: boolean;
  calibrationSaveError: boolean;
}>();
defineEmits<{ calibrate: []; clear: []; pause: [] }>();
const state = useTelemetry();
const robot = ref<InstanceType<typeof RobotView>>();
const lastImu = computed(() => state.chart.at(-1));
const value = (values: number[] | undefined, i: number) =>
  values?.[i]?.toFixed(2) ?? "—";
</script>
<template>
  <section class="workbench" aria-label="姿态与 IMU 单屏工作台">
    <div class="bench-toolbar">
      <div class="bench-title">
        <h1>姿态与 IMU</h1>
        <span>{{ state.info?.name || "MicroDuck" }}</span>
      </div>
      <div class="bench-live">
        <span :class="{ bad: state.poseState !== '实时' }">{{
          state.poseState
        }}</span
        ><b>{{ state.hz.toFixed(1) }} <small>Hz</small></b
        ><span
          >样本
          {{
            Number.isFinite(state.age) ? Math.round(state.age) : "—"
          }}
          ms</span
        >
      </div>
      <button class="button compact" @click="$emit('pause')">
        {{ state.paused ? "▶ 恢复显示" : "Ⅱ 暂停显示" }}
      </button>
    </div>
    <div v-if="state.error || state.paused" class="bench-alert" role="status">
      {{ state.error || "显示已暂停 · 后台继续接收数据" }}
    </div>
    <div class="bench-grid">
      <section class="panel bench-model">
        <div class="bench-panel-head">
          <h2>3D 姿态</h2>
          <span class="subtle-tag">{{ state.source }}</span>
        </div>
        <div class="bench-model-stage">
          <RobotView
            :key="String(state.sensorOnly)"
            ref="robot"
            :quaternion="modelOrientation"
            :paused="state.paused"
          />
          <span class="bench-model-state">{{
            state.paused
              ? "显示暂停"
              : state.sensorOnly && !calibrated
                ? "参考姿态 · 待标定"
                : state.poseState === "实时"
                  ? "相对姿态 · 实时"
                  : `姿态${state.poseState}`
          }}</span>
          <button class="bench-home" @click="robot?.home()">↺ 回正视角</button>
        </div>
        <div class="bench-calibration">
          <button
            class="button compact"
            :disabled="!canCalibrate"
            @click="$emit('calibrate')"
          >
            {{ calibrated ? "重新标定初始姿态" : "标定初始姿态" }}
          </button>
          <button
            class="text-button"
            :disabled="!calibrated || state.paused"
            @click="$emit('clear')"
          >
            清除标定
          </button>
          <span>{{
            calibrationSaveError ? "主板同步失败" : calibrated ? "已保存到主板 " + calibrationTime : "摆正实物并静止后标定"
          }}</span>
        </div>
        <div class="bench-angles">
          <div
            v-for="(name, i) in ['Roll 横滚', 'Pitch 俯仰', 'Yaw 偏航']"
            :key="name"
          >
            <span>{{ name }}</span
            ><strong
              >{{ angles?.[i]?.toFixed(2) ?? "—" }}<small>°</small></strong
            >
          </div>
        </div>
        <div class="bench-quaternion">
          <span>{{ calibrated ? "相对" : "原始" }}四元数</span
          ><code v-for="(axis, i) in ['x', 'y', 'z', 'w']" :key="axis"
            >{{ axis }} <b>{{ quaternion?.[i]?.toFixed(3) ?? "—" }}</b></code
          >
        </div>
        <div class="bench-model-note">
          IMU 相对旋转 · {{ jointPose.calibratedCount }} 个关节已标定跟随<br />关节标定由主板共享；IMU 会话重启后需重新归零。
        </div>
      </section>
      <section class="panel bench-imu">
        <div class="bench-panel-head">
          <h2>IMU</h2>
          <span
            class="subtle-tag"
            :class="{ bad: state.imuState !== '实时' }"
            >{{ state.imuState }}</span
          >
        </div>
        <div class="bench-axis-legend">
          <span>X</span><span>Y</span><span>Z</span><small>最近 60 秒</small>
        </div>
        <section class="bench-signal">
          <div class="bench-signal-heading">
            <h3>角速度</h3>
            <small>rad/s</small>
          </div>
          <div class="bench-axis-values">
            <b v-for="i in 3" :key="i"><small>{{ ['X', 'Y', 'Z'][i - 1] }}</small> {{ value(lastImu?.gyro, i - 1) }}</b>
          </div>
          <SignalChart :samples="state.chart" kind="gyro" />
        </section>
        <section class="bench-signal">
          <div class="bench-signal-heading">
            <h3>加速度</h3>
            <small>m/s²</small>
          </div>
          <div class="bench-axis-values">
            <b v-for="i in 3" :key="i"><small>{{ ['X', 'Y', 'Z'][i - 1] }}</small> {{ value(lastImu?.accel, i - 1) }}</b>
          </div>
          <SignalChart :samples="state.chart" kind="accel" />
        </section>
        <div class="bench-imu-note">
          原始 R / P / Y
          <strong>{{
            state.angles?.map((v) => v.toFixed(1) + "°").join(" / ") || "—"
          }}</strong
          ><span>传感器坐标 · 未完成安装校准</span>
        </div>
      </section>
      <ServoPanel compact />
    </div>
    <div class="bench-statusbar">
      <span>观测模式 · 不发送运动指令</span
      ><span>15 个舵机 · 已标定关节跟随反馈（嘴部为简化模型）</span>
    </div>
  </section>
</template>

<style>
.workbench {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.bench-toolbar {
  height: 44px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 18px;
}
.bench-title {
  display: flex;
  align-items: baseline;
  gap: 14px;
}
.bench-title h1 {
  font-size: 18px;
  margin: 0;
}
.bench-title > span {
  font-size: 10px;
  color: #84947d;
}
.bench-live {
  margin-left: auto;
  display: flex;
  gap: 14px;
  align-items: center;
  font-size: 10px;
  color: #829577;
}
.bench-live b {
  font-size: 16px;
  color: #456a4e;
  font-weight: 500;
}
.bench-live small {
  font-size: 9px;
}
.bench-alert {
  background: #faf0d7;
  border: 1px solid #eadcb4;
  border-radius: 4px;
  padding: 5px 10px;
  font-size: 10px;
  color: #987738;
  flex-shrink: 0;
}
.bench-grid {
  display: grid;
  grid-template-columns: minmax(0, 3fr) minmax(0, 4fr) minmax(0, 3fr);
  grid-template-areas: "imu model servos";
  gap: 10px;
  flex: 1;
  min-height: 0;
}
.bench-grid > .servo-panel { grid-area: servos; }
.bench-grid > .panel {
  margin: 0;
  min-width: 0;
  min-height: 0;
}
.bench-panel-head {
  height: 43px;
  flex-shrink: 0;
  padding: 0 13px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid #e9eee4;
}
.bench-panel-head h2 {
  font-size: 12px;
}
.bench-model {
  grid-area: model;
  display: flex;
  flex-direction: column;
}
.bench-model-stage {
  position: relative;
  flex: 1;
  min-height: 70px;
  background: radial-gradient(ellipse at center, #edf3e8, #fafcf7);
}
.bench-model-state {
  position: absolute;
  left: 12px;
  top: 13px;
  font-size: 9px;
  color: #7b9069;
  background: #ffffffc9;
  border: 1px solid #dfe7d7;
  border-radius: 4px;
  padding: 5px 7px;
}
.bench-home {
  position: absolute;
  bottom: 10px;
  right: 12px;
  font-size: 10px;
  background: #ffffffbd;
  border: 1px solid #dce5d2;
  border-radius: 4px;
  color: #849573;
  padding: 5px 7px;
}
.bench-calibration {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  padding: 10px 12px;
  border-top: 1px solid #e9eee4;
  flex-shrink: 0;
}
.bench-calibration > span {
  font-size: 9px;
  color: #91a17f;
  width: 100%;
}
.bench-angles {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 5px;
  padding: 10px 12px;
  background: #f7f9f3;
  flex-shrink: 0;
}
.bench-angles span {
  display: block;
  font-size: 9px;
  color: #8b9a7c;
}
.bench-angles strong {
  font-weight: 500;
  display: block;
  font-size: 20px;
  margin-top: 6px;
  font-variant-numeric: tabular-nums;
}
.bench-angles small {
  font-size: 11px;
  color: #9caa8b;
  margin-left: 3px;
}
.bench-quaternion {
  padding: 11px 12px;
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 5px;
  flex-shrink: 0;
}
.bench-quaternion > span {
  grid-column: 1/-1;
  font-size: 9px;
  color: #8d9c7f;
  margin-bottom: 3px;
}
.bench-quaternion code {
  font-size: 9px;
  color: #93a582;
}
.bench-quaternion b {
  font-weight: 500;
  color: #607b4d;
}
.bench-model-note {
  font-size: 9px;
  line-height: 1.7;
  padding: 0 12px 11px;
  color: #99a68c;
  flex-shrink: 0;
}
.bench-imu {
  grid-area: imu;
  display: flex;
  flex-direction: column;
}
.bench-axis-legend {
  display: flex;
  align-items: center;
  gap: 13px;
  padding: 12px 12px 5px;
  font-size: 10px;
  flex-shrink: 0;
}
.bench-axis-legend span:first-child,
.bench-axis-values b:first-child {
  color: #37816a;
}
.bench-axis-legend span:nth-child(2),
.bench-axis-values b:nth-child(2) {
  color: #dba855;
}
.bench-axis-legend span:nth-child(3),
.bench-axis-values b:nth-child(3) {
  color: #7196c2;
}
.bench-axis-legend small {
  margin-left: auto;
  color: #9aa78c;
  font-size: 8px;
}
.bench-signal {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  padding-top: 10px;
}
.bench-signal + .bench-signal {
  border-top: 1px solid #e9eee4;
}
.bench-signal-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  flex-shrink: 0;
}
.bench-signal-heading h3 {
  font-weight: 500;
  margin: 0;
  font-size: 11px;
}
.bench-signal-heading small {
  font-size: 9px;
  color: #99a78b;
}
.bench-axis-values {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 3px;
  padding: 10px 12px 0;
  flex-shrink: 0;
}
.bench-axis-values b {
  font-size: 14px;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
}
.bench-axis-values small { font-size: 9px; font-weight: 400; }
.bench-signal .signal-chart {
  height: auto;
  flex: 1;
  min-height: 45px;
  padding: 0;
}
.bench-imu-note {
  padding: 12px;
  font-size: 9px;
  color: #92a180;
  border-top: 1px solid #e9eee4;
  flex-shrink: 0;
}
.bench-imu-note strong {
  display: block;
  font-weight: 500;
  color: #688151;
  font-size: 11px;
  margin: 6px 0;
}
.bench-imu-note span {
  font-size: 8px;
}
.bench-statusbar {
  display: flex;
  justify-content: space-between;
  font-size: 9px;
  color: #9ba88f;
  flex-shrink: 0;
  height: 17px;
  align-items: center;
}
@media (max-height: 720px) {
  .bench-calibration {
    padding: 7px 10px;
  }
  .bench-angles {
    padding: 8px 10px;
  }
  .bench-angles strong {
    font-size: 18px;
    margin-top: 4px;
  }
  .bench-quaternion {
    padding: 8px 10px;
  }
  .bench-model-note {
    font-size: 8px;
    padding-bottom: 7px;
  }
  .bench-panel-head {
    height: 36px;
  }
  .bench-imu-note {
    padding: 8px;
  }
  .bench-toolbar {
    height: 35px;
  }
}
@media (max-width: 1100px) {
  .bench-grid { gap: 7px; }
  .bench-title > span { display: none; }
  .bench-calibration { padding: 7px; gap: 5px; }
  .bench-axis-values { padding: 10px 7px 0; }
  .bench-axis-values b { font-size: 12px; }
  .bench-quaternion { padding: 8px; gap: 3px; }
}
</style>
