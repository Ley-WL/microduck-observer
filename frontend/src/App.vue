<script setup lang="ts">
import { computed, onMounted, ref, shallowRef, watch, nextTick } from "vue";
import { useTelemetry } from "./store";
import RobotView from "./components/RobotView.vue";
import SignalChart from "./components/SignalChart.vue";
import PoseWorkbench from "./components/PoseWorkbench.vue";
import { euler, type Sample } from "./protocol";
import { relativeQuaternion } from "./calibration";
const state = useTelemetry();
const page = ref("姿态与 IMU"),
  kind = ref<"gyro" | "accel">("gyro"),
  level = ref("ALL"),
  query = ref(""),
  follow = ref(true);
const robot = ref<InstanceType<typeof RobotView>>(),
  logElement = ref<HTMLDivElement>();
const frozenPose = shallowRef<number[] | null>(null),
  frozenAngles = shallowRef<number[] | null>(null);
const shownLogs = shallowRef<Sample[]>([]);
const initialOrientation = shallowRef<number[] | null>(null);
const calibrationTime = ref("");
const canCalibrate = computed(
  () =>
    state.connection === "在线" &&
    state.poseState === "实时" &&
    !state.paused &&
    !!state.orientation?.valid,
);
const displayOrientation = computed(() => {
  if (!state.orientation?.valid) return null;
  const raw = state.orientation.data.quaternion;
  return initialOrientation.value
    ? relativeQuaternion(initialOrientation.value, raw)
    : raw;
});
const modelOrientation = computed(() =>
  state.sensorOnly && !initialOrientation.value
    ? [0, 0, 0, 1]
    : quaternion.value,
);
function calibrateInitial() {
  if (!canCalibrate.value) return;
  initialOrientation.value = [...state.orientation!.data.quaternion];
  calibrationTime.value = new Date().toLocaleTimeString("zh-CN", {
    hour12: false,
  });
}
function clearCalibration() {
  initialOrientation.value = null;
  calibrationTime.value = "";
}
watch(
  [
    () => state.info?.bootId,
    () => state.orientation?.data.frame,
    () => state.orientation?.source,
  ],
  () => {
    clearCalibration();
    frozenPose.value = null;
    frozenAngles.value = null;
    state.paused = false;
  },
);
const quaternion = computed(() =>
  state.paused ? frozenPose.value : displayOrientation.value,
);
const angles = computed(() =>
  state.paused
    ? frozenAngles.value
    : displayOrientation.value
      ? euler(displayOrientation.value)
      : null,
);
const filteredLogs = computed(() =>
  shownLogs.value.filter(
    (x) =>
      (level.value === "ALL" || x.data.level === level.value) &&
      `${x.data.module} ${x.data.message}`
        .toLowerCase()
        .includes(query.value.toLowerCase()),
  ),
);
watch(
  () => state.logs,
  async (logs) => {
    if (!follow.value) return;
    shownLogs.value = logs;
    await nextTick();
    if (logElement.value)
      logElement.value.scrollTop = logElement.value.scrollHeight;
  },
);
watch(follow, (v) => {
  if (v) shownLogs.value = state.logs;
});
function pause() {
  if (!state.paused) {
    frozenPose.value = quaternion.value;
    frozenAngles.value = angles.value;
  }
  state.paused = !state.paused;
}
function exportLogs() {
  const text = filteredLogs.value.map((x) => JSON.stringify(x)).join("\n");
  const url = URL.createObjectURL(
    new Blob([text], { type: "application/x-ndjson" }),
  );
  const a = document.createElement("a");
  a.href = url;
  a.download = `microduck-logs-${Date.now()}.jsonl`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
function time(ms: number) {
  return Number.isFinite(ms)
    ? new Date(ms).toLocaleTimeString("zh-CN", { hour12: false })
    : "—";
}
onMounted(() => state.connect());
</script>
<template>
  <div class="shell" :class="{ 'workbench-shell': page === '姿态与 IMU' }">
    <aside class="sidebar">
      <a class="brand" href="#" @click.prevent="page = '总览'"
        ><span class="brand-mark">μ</span
        ><span>MicroDuck<small>DEVELOPER STUDIO</small></span></a
      >
      <div class="workspace-label">
        工作空间
        <span>{{ state.info?.source === "hardware" ? "ROBOT" : "LOCAL" }}</span>
      </div>
      <nav>
        <button
          v-for="(label, i) in ['总览', '姿态与 IMU', '日志', '传感器', '画面']"
          :key="label" :title="label"
          :class="{ active: page === label }"
          @click="page = label"
        >
          <span class="nav-symbol">{{ ["◫", "⌁", "≡", "◉", "▣"][i] }}</span
          >{{ label }}<span v-if="i > 2" class="later">后续</span>
        </button>
      </nav>
      <div class="sidebar-note">
        <span class="tiny-dot"></span>
        {{
          state.info?.source === "hardware" ? "主控实机观测" : "独立调试环境"
        }}
        <p>连接数据，理解每一次运动。</p>
        <small>OBSERVER / v0.3.0</small>
      </div>
    </aside>
    <div class="main-shell">
      <header>
        <div class="breadcrumb">
          工作空间 <span>/</span> MicroDuck Lab <span>/</span> <b>{{ page }}</b>
        </div>
        <div class="header-right">
          <span class="source-badge">{{ state.source }}</span
          ><span class="status" :class="{ online: state.connection === '在线' }"
            ><i></i>{{ state.connection }}</span
          >
        </div>
      </header>
      <main>
        <PoseWorkbench
          v-if="page === '姿态与 IMU'"
          :model-orientation="modelOrientation"
          :quaternion="quaternion"
          :angles="angles"
          :calibrated="Boolean(initialOrientation)"
          :calibration-time="calibrationTime"
          :can-calibrate="canCalibrate"
          @calibrate="calibrateInitial"
          @clear="clearCalibration"
          @pause="pause"
        />
        <template v-else>
          <div class="page-heading">
            <div>
              <div class="eyebrow">ROBOT OBSERVABILITY</div>
              <h1>
                {{ page === "总览" ? "让机器人的状态，一目了然。" : page }}
              </h1>
              <p>
                {{
                  page === "总览"
                    ? "姿态、传感器与运行日志，在同一个时间线上。"
                    : "MicroDuck · 实时数据观测工作空间"
                }}
              </p>
            </div>
            <button
              class="button"
              :class="{ selected: state.paused }"
              @click="pause"
            >
              {{ state.paused ? "▶ 恢复显示" : "Ⅱ 暂停显示" }}
            </button>
          </div>
          <section class="connection-bar">
            <span class="device-icon">⌘</span>
            <div class="device-name">
              {{ state.info?.name || "MicroDuck Lab"
              }}<small>{{
                state.info?.source === "simulation"
                  ? "独立模拟服务 · 未连接真实硬件"
                  : "遥测服务"
              }}</small>
            </div>
            <div class="endpoint-control">
              <label for="endpoint">服务地址</label
              ><input
                id="endpoint"
                v-model="state.endpoint"
                :disabled="
                  ['在线', '连接中', '重连中'].includes(state.connection)
                "
                @keyup.enter="state.connect()"
              /><button
                class="button compact"
                @click="
                  state.connection === '离线' || state.connection === '不兼容'
                    ? state.connect()
                    : state.disconnect()
                "
              >
                {{
                  state.connection === "离线" || state.connection === "不兼容"
                    ? "连接"
                    : "断开"
                }}
              </button>
            </div>
          </section>
          <div v-if="state.error" class="notice warning" role="alert">
            {{ state.error }}
          </div>
          <div v-if="state.paused" class="notice warning">
            显示已暂停 ·
            姿态与曲线保持当前画面，后台仍接收数据。日志可单独暂停浏览。
          </div>

          <template v-if="page === '总览'">
            <div class="metrics">
              <article>
                <span>姿态数据</span
                ><strong :class="{ bad: state.poseState !== '实时' }"
                  >{{ state.poseState
                  }}<i
                    class="metric-dot"
                    v-if="state.poseState === '实时'"
                  ></i></strong
                ><small
                  >{{ state.sensorOnly ? "传感器朝向" : "身体朝向" }} ·
                  {{
                    state.orientation?.source === "simulation"
                      ? "模拟"
                      : state.orientation?.source === "hardware"
                        ? "实机"
                        : state.orientation?.source === "replay"
                          ? "回放"
                          : "未提供"
                  }}</small
                >
              </article>
              <article>
                <span>姿态接收频率</span
                ><strong>{{ state.hz.toFixed(1) }} <em>Hz</em></strong
                ><small>浏览器每秒接收的样本数</small>
              </article>
              <article>
                <span>姿态样本年龄</span
                ><strong
                  >{{
                    Number.isFinite(state.age) ? Math.round(state.age) : "—"
                  }}
                  <em>ms</em></strong
                ><small>不含未知单程网络延迟</small>
              </article>
              <article>
                <span>关节反馈</span><strong>— <em>/ 14</em></strong
                ><small>当前未提供 · 使用参考姿势</small>
              </article>
            </div>
            <div class="pose-layout">
              <section class="panel model-panel">
                <div class="panel-title">
                  <div>
                    <span class="panel-icon">◇</span>
                    <h2>鸭子姿态预览</h2>
                    <span class="subtle-tag">3D VIEW</span>
                  </div>
                  <div class="pose-actions">
                    <button
                      class="button compact"
                      :disabled="!canCalibrate"
                      @click="calibrateInitial"
                      title="摆好鸭子的初始姿态，保持静止后点击；仅设定当前页面的显示零点"
                    >
                      {{
                        initialOrientation ? "重新标定初始姿态" : "标定初始姿态"
                      }}
                    </button>
                    <button
                      v-if="initialOrientation"
                      class="text-button"
                      :disabled="state.paused"
                      @click="clearCalibration"
                    >
                      清除标定
                    </button>
                    <button class="text-button" @click="robot?.home()">
                      ↺ 回正视角
                    </button>
                  </div>
                </div>
                <div class="model-stage">
                  <RobotView
                    :key="String(state.sensorOnly)"
                    ref="robot"
                    :quaternion="modelOrientation"
                    :paused="state.paused"
                  />
                  <div class="model-overlay">
                    <span class="overlay-tag">{{
                      state.paused
                        ? "显示暂停"
                        : state.sensorOnly && !initialOrientation
                          ? "参考姿态 · 待标定"
                          : state.poseState === "实时"
                            ? state.sensorOnly
                              ? "相对初始姿态 · 实时"
                              : "身体朝向实时"
                            : `姿态${state.poseState}`
                    }}</span
                    ><span>{{
                      state.sensorOnly
                        ? "IMU 相对旋转预览 · 关节为参考姿势"
                        : "关节为参考姿势"
                    }}</span>
                  </div>
                  <div class="axis-key">
                    <b class="axis-x">X</b>
                    {{ state.sensorOnly ? "传感器轴" : "前" }}
                    <b class="axis-y">Y</b>
                    {{ state.sensorOnly ? "传感器轴" : "左" }}
                    <b class="axis-z">Z</b>
                    {{ state.sensorOnly ? "传感器轴" : "上" }}
                  </div>
                  <div class="model-caption">拖动旋转 · 滚轮缩放</div>
                </div>
                <div class="model-footer">
                  <span
                    ><i class="tiny-dot"></i>
                    {{
                      state.orientation?.source === "simulation"
                        ? "SIMULATED POSE"
                        : state.sensorOnly
                          ? "BNO085 · LIVE SENSOR"
                          : "POSE VIEW"
                    }}</span
                  ><span>{{
                    initialOrientation
                      ? "显示坐标：初始姿态参考系"
                      : state.sensorOnly
                        ? "坐标系：传感器 → IMU 参考系"
                        : "坐标系：身体 → 世界 · 右手系"
                  }}</span>
                </div>
              </section>
              <section class="panel attitude-panel">
                <div class="panel-title">
                  <h2>
                    {{ initialOrientation ? "相对初始姿态" : "原始姿态读数" }}
                  </h2>
                  <span class="subtle-tag">DEG</span>
                </div>
                <div
                  class="angle-row"
                  v-for="(name, i) in [
                    'Roll / 横滚',
                    'Pitch / 俯仰',
                    'Yaw / 偏航',
                  ]"
                  :key="name"
                >
                  <div>
                    <span :class="'axis-' + ['x', 'y', 'z'][i]">{{
                      ["X", "Y", "Z"][i]
                    }}</span
                    >{{ name }}
                  </div>
                  <strong
                    >{{ angles ? angles[i].toFixed(2) : "—" }}<em>°</em></strong
                  >
                  <div class="angle-track">
                    <i
                      :style="{
                        width:
                          Math.min(
                            50,
                            (Math.abs(angles?.[i] || 0) / 180) * 50,
                          ) + '%',
                        left: (angles?.[i] || 0) < 0 ? 'auto' : '50%',
                        right: (angles?.[i] || 0) < 0 ? '50%' : 'auto',
                        background: ['#37816a', '#dba855', '#7196c2'][i],
                      }"
                    ></i>
                  </div>
                </div>
                <div class="quaternion">
                  <span>四元数 <small>XY ZW / 单位四元数</small></span>
                  <div v-for="(axis, i) in ['x', 'y', 'z', 'w']" :key="axis">
                    <b>{{ axis }}</b
                    ><code>{{
                      quaternion ? quaternion[i].toFixed(4) : "—"
                    }}</code>
                  </div>
                </div>
                <div class="calibration-note">
                  <template v-if="initialOrientation"
                    >初始姿态已标定 · {{ calibrationTime
                    }}<small
                      >仅当前页面有效；刷新或服务重启需重新标定。</small
                    ></template
                  >
                  <template v-else
                    >将鸭子摆好并保持静止，点击“标定初始姿态”。<small
                      >当前读数为原始朝向，实机鸭子保持参考姿态。</small
                    ></template
                  >
                  <small>显示零点不等同于安装轴向校准；不会修改硬件。</small>
                  <!-- The original measurement remains in the telemetry store. -->
                  <span class="raw-orientation"
                    >原始 R/P/Y：{{
                      state.angles
                        ?.map((v) => v.toFixed(1) + "°")
                        .join(" / ") || "—"
                    }}</span
                  >
                  <small>
                    {{
                      state.orientation?.source === "simulation"
                        ? "模拟安装坐标 · identity"
                        : "安装校准：" +
                          (state.orientation?.data.calibrationId ||
                            "未完成，当前为原始 IMU 朝向")
                    }}</small
                  >
                </div>
              </section>
            </div>
            <section class="panel chart-panel">
              <div class="panel-title">
                <div>
                  <h2>IMU 实时趋势</h2>
                  <span
                    class="subtle-tag"
                    :class="{ bad: state.imuState !== '实时' }"
                    >{{ state.imuState }}</span
                  ><span class="muted"
                    >最近 60 秒 · {{ kind === "gyro" ? "rad/s" : "m/s²" }}</span
                  >
                </div>
                <div class="chart-controls">
                  <span class="legend"><i></i>X <i></i>Y <i></i>Z</span>
                  <div class="segmented">
                    <button
                      :class="{ active: kind === 'gyro' }"
                      @click="kind = 'gyro'"
                    >
                      角速度</button
                    ><button
                      :class="{ active: kind === 'accel' }"
                      @click="kind = 'accel'"
                    >
                      加速度
                    </button>
                  </div>
                </div>
              </div>
              <SignalChart :samples="state.chart" :kind="kind" />
            </section>
          </template>

          <section
            v-if="page === '总览' || page === '日志'"
            class="panel logs-panel"
          >
            <div class="panel-title">
              <div>
                <h2>运行日志</h2>
                <span class="count">{{ state.logs.length }}</span
                ><span v-if="!follow" class="subtle-tag">浏览已暂停</span>
              </div>
              <div class="log-actions">
                <button class="text-button" @click="follow = !follow">
                  {{ follow ? "Ⅱ 暂停浏览" : "▶ 继续浏览" }}</button
                ><button class="text-button" @click="exportLogs">
                  ↓ 导出筛选结果
                </button>
              </div>
            </div>
            <div class="log-toolbar">
              <div class="log-filters">
                <button
                  v-for="item in ['ALL', 'INFO', 'WARN', 'ERROR']"
                  :class="{ active: level === item }"
                  @click="level = item"
                >
                  {{ item === "ALL" ? "全部" : item }}
                </button>
              </div>
              <input
                v-model="query"
                aria-label="搜索日志"
                placeholder="搜索模块或日志内容…"
              />
            </div>
            <div v-if="state.logGap" class="notice warning">
              服务日志缓存发生缺失，当前显示的历史不完整。
            </div>
            <div
              ref="logElement"
              class="log-list"
              :class="{ expanded: page === '日志' }"
            >
              <div v-if="!filteredLogs.length" class="empty-logs">
                {{
                  state.logs.length ? "没有符合筛选条件的日志" : "等待服务日志…"
                }}
              </div>
              <div
                v-for="item in filteredLogs.slice(-500)"
                :key="item.bootId + ':' + item.seq"
                class="log-row"
              >
                <time>{{ time(item.data.eventTimeMs) }}</time
                ><span
                  class="log-level"
                  :class="item.data.level.toLowerCase()"
                  >{{ item.data.level }}</span
                ><span class="log-module">{{ item.data.module }}</span
                ><span class="log-message">{{ item.data.message }}</span>
              </div>
            </div>
            <div class="log-footer">
              <span
                >{{ follow ? "● 自动跟随" : "○ 浏览暂停" }} · 缓存上限 5,000
                条，显示最近 500 条匹配记录</span
              ><span>已拒绝异常消息 {{ state.rejected }}</span>
            </div>
          </section>

          <section
            v-if="page === '传感器' || page === '画面'"
            class="panel future-panel"
          >
            <div class="future-icon">{{ page === "画面" ? "▣" : "◉" }}</div>
            <div class="eyebrow">
              {{ page === "画面" ? "CAMERA STREAM" : "DISTANCE SENSING" }}
            </div>
            <h2>
              {{ page === "画面" ? "等待摄像头数据源" : "等待 ToF 数据源" }}
            </h2>
            <p>
              {{
                page === "画面"
                  ? "后续通过独立媒体通道接入视频，展示画面、帧率与流状态。"
                  : "后续接入单点距离或多区深度热图，显示量程、有效性和测量时间。"
              }}
            </p>
            <span class="subtle-tag">当前版本未接入</span>
          </section>

          <section
            v-if="state.info?.capabilities?.scenarios"
            class="scenario-bar"
          >
            <div>
              <strong>模拟场景</strong
              ><span>验证界面在不同数据状态下的表现</span>
            </div>
            <div>
              <button
                v-for="s in [
                  { id: 'motion', name: '组合姿态' },
                  { id: 'steady', name: '静止' },
                  { id: 'imu_pause', name: 'IMU 停更' },
                  { id: 'invalid', name: '无效姿态' },
                  { id: 'log_burst', name: '日志突发' },
                ]"
                :disabled="state.connection !== '在线'"
                @click="state.scenario(s.id)"
              >
                {{ s.name }}
              </button>
            </div>
          </section>
          <footer>
            <span>MICRODUCK <b> / </b> 观测先行，调试有据。</span
            ><span>本平台 v0.2 不发送运动控制指令</span>
          </footer>
        </template>
      </main>
    </div>
  </div>
</template>
