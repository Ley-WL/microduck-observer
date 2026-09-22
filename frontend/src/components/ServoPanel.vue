<script setup lang="ts">
import { computed } from "vue";
import { useTelemetry } from "../store";
defineProps<{ compact?: boolean }>();
const state = useTelemetry();
const rows = computed(() => state.joints?.data.servos || []);
const fresh = computed(() => state.connection === "在线" && !!state.joints?.valid && state.jointsAge < 1500);
const online = computed(() => fresh.value ? rows.value.filter((r: any) => r.online && r.ageMs + state.jointsAge < 1500) : []);
const summary = computed(() => ({
  voltage: online.value.length ? (online.value.reduce((n: number, r: any) => n+r.voltage, 0)/online.value.length).toFixed(1) : "—",
  temperature: online.value.length ? Math.max(...online.value.map((r: any) => r.temperature)) : "—",
  faults: online.value.length ? online.value.filter((r: any) => r.fault !== 0).length : "—",
}));
function status(id: number) {
  if (!state.joints?.data.configuredIds.includes(id)) return "未接入";
  if (state.paused) return "已暂停";
  if (!fresh.value) return "已过期";
  const r = rows.value.find((r: any) => r.id === id);
  return !r?.online ? "无应答" : r.ageMs + state.jointsAge >= 1500 ? "已过期" : r.fault ? "异常" : "在线";
}
function value(id: number, field: string) {
  const r = online.value.find((r: any) => r.id === id);
  if (!r) return "—";
  if (field === "age") return Math.round(r.ageMs + state.jointsAge);
  if (field === "voltage") return r.voltage.toFixed(1);
  if (field === "torque") return r.torque === 0 ? "关闭" : r.torque === 1 ? "开启" : "模式 " + r.torque;
  return r[field] ?? "—";
}
// Physical IDs match the existing servo-web tool; no bus access in this panel.
const groups = [
  {
    name: "左腿",
    range: "20–24",
    joints: [
      { id: 20, name: "左髋 · 偏航", axis: "yaw" },
      { id: 21, name: "左髋 · 横滚", axis: "roll" },
      { id: 22, name: "左髋 · 俯仰", axis: "pitch" },
      { id: 23, name: "左膝", axis: "" },
      { id: 24, name: "左踝", axis: "" },
    ],
  },
  {
    name: "头颈与嘴",
    range: "30–34",
    joints: [
      { id: 30, name: "颈部 · 俯仰", axis: "pitch" },
      { id: 31, name: "头部 · 俯仰", axis: "pitch" },
      { id: 32, name: "头部 · 偏航", axis: "yaw" },
      { id: 33, name: "头部 · 横滚", axis: "roll" },
      { id: 34, name: "嘴", axis: "" },
    ],
  },
  {
    name: "右腿",
    range: "10–14",
    joints: [
      { id: 10, name: "右髋 · 偏航", axis: "yaw" },
      { id: 11, name: "右髋 · 横滚", axis: "roll" },
      { id: 12, name: "右髋 · 俯仰", axis: "pitch" },
      { id: 13, name: "右膝", axis: "" },
      { id: 14, name: "右踝", axis: "" },
    ],
  },
];
</script>

<template>
  <section
    class="panel servo-panel"
    :class="{ 'servo-compact': compact }"
    aria-labelledby="servo-title"
  >
    <div class="panel-title">
      <div>
        <h2 id="servo-title">舵机反馈</h2>
        <span class="count">15 个舵机</span>
      </div>
      <span class="subtle-tag">{{ state.joints ? (state.paused ? "显示暂停" : fresh ? "只读采集" : "反馈过期") : "反馈未接入" }}</span>
    </div>
    <div class="servo-summary">
      <div>
        <span>在线数量</span><strong>{{ state.joints ? online.length : "—" }} <small>/ 15</small></strong>
      </div>
      <div>
        <span>平均电压</span><strong>{{ summary.voltage }} <small>V</small></strong>
      </div>
      <div>
        <span>最高温度</span><strong>{{ summary.temperature }} <small>°C</small></strong>
      </div>
      <div><span>故障数量</span><strong>{{ summary.faults }}</strong></div>
      <p>只读反馈，不发送运动、扭矩或校准指令。</p>
    </div>
    <div
      class="servo-table-scroll"
      role="region"
      aria-label="15 个舵机反馈"
      tabindex="0"
    >
      <table class="servo-table">
        <caption>
          飞特 HD-1910 · 未采集到数据的项目显示 —
        </caption>
        <thead>
          <tr>
            <th scope="col">ID / 关节</th>
            <th v-if="!compact" scope="col">在线状态</th>
            <th v-if="!compact" scope="col">
              实测位置<small>编码器步数</small>
            </th>
            <th scope="col">编码器<small>步</small></th>
            <th scope="col">电压<small>V</small></th>
            <th scope="col">温度<small>°C</small></th>
            <th scope="col">电流<small>原始值</small></th>
            <th scope="col">负载<small>原始值</small></th>
            <th v-if="!compact" scope="col">扭矩状态</th>
            <th v-if="!compact" scope="col">故障</th>
            <th v-if="!compact" scope="col">样本年龄<small>ms</small></th>
            <th v-if="compact" scope="col">状态</th>
          </tr>
        </thead>
        <tbody v-for="group in groups" :key="group.name">
          <tr class="servo-group">
            <th :colspan="compact ? 7 : 11" scope="rowgroup">
              {{ group.name }} <span>ID {{ group.range }} · 5 个</span>
            </th>
          </tr>
          <tr v-for="joint in group.joints" :key="joint.id" class="servo-row">
            <th scope="row">
              <span class="servo-id">{{ joint.id }}</span
              >{{ joint.name }}
            </th>
            <td v-if="!compact">
              <span class="servo-unavailable">{{ status(joint.id) }}</span>
            </td>
            <td
              v-for="field in compact
                ? ['position', 'voltage', 'temperature', 'currentRaw', 'load']
                : [
                    'position',
                    'angle',
                    'voltage',
                    'temperature',
                    'currentRaw',
                    'load',
                    'torque',
                    'fault',
                    'age',
                  ]"
              :key="field"
              class="servo-empty"
            >
              {{ value(joint.id, field) }}
            </td>
            <td v-if="compact">
              <span class="servo-unavailable">{{ status(joint.id) }}</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-if="compact" class="servo-footnote">
      {{ state.joints?.data.error || "只读观测 · 未配置的舵机显示未接入" }}<br />编码器未换算关节角度；电流单位待核实。
    </div>
    <div v-else class="servo-footnote">
      未接入不代表离线或故障。15 个实物舵机包含嘴部（ID 34）；当前 3D 模型仅有
      14 个关节，嘴部反馈单独展示。
    </div>
  </section>
</template>

<style scoped>
.servo-panel {
  margin-bottom: 20px;
}
.servo-summary {
  display: flex;
  align-items: center;
  gap: 32px;
  padding: 18px 20px;
  border-bottom: 1px solid #edf1e9;
  flex-wrap: wrap;
}
.servo-summary > div {
  min-width: 82px;
}
.servo-summary span {
  display: block;
  font-size: 10px;
  color: #82917b;
}
.servo-summary strong {
  display: block;
  font-size: 21px;
  font-weight: 500;
  margin-top: 7px;
}
.servo-summary small {
  font-size: 11px;
  color: #9aa58f;
  font-weight: 400;
}
.servo-summary p {
  margin: 0 0 0 auto;
  max-width: 270px;
  line-height: 1.8;
  font-size: 11px;
  color: #8c9982;
}
.servo-table-scroll {
  overflow-x: auto;
}
.servo-table {
  border-collapse: collapse;
  width: 100%;
  min-width: 1000px;
  text-align: left;
  font-size: 11px;
  white-space: nowrap;
}
.servo-table caption {
  text-align: left;
  font-size: 10px;
  color: #8b9980;
  padding: 12px 20px;
}
.servo-table th,
.servo-table td {
  padding: 11px 12px;
  border-bottom: 1px solid #edf1e9;
}
.servo-table th:first-child,
.servo-table td:first-child {
  padding-left: 20px;
}
.servo-table thead th {
  font-size: 10px;
  color: #819175;
  font-weight: 500;
  background: #fafbf8;
}
.servo-table thead small {
  display: block;
  font-size: 9px;
  color: #a0ab95;
  font-weight: 400;
  margin-top: 4px;
}
.servo-group th {
  background: #edf3e9;
  color: #5d7650;
  font-weight: 600;
  font-size: 11px;
}
.servo-group span {
  font-size: 9px;
  color: #96a489;
  font-weight: 400;
  margin-left: 12px;
}
.servo-row th {
  font-weight: 500;
  color: #5d7051;
}
.servo-row:hover {
  background: #fafcf7;
}
.servo-id {
  display: inline-block;
  width: 25px;
  color: #91a47d;
  font-family: Consolas, monospace;
  margin-right: 9px;
}
.servo-unavailable {
  display: inline-block;
  background: #f0f3ed;
  border: 1px solid #e3e9dd;
  border-radius: 4px;
  padding: 3px 7px;
  color: #8f9c81;
  font-size: 9px;
}
.servo-empty {
  color: #a7b09e;
}
.servo-footnote {
  padding: 13px 20px;
  font-size: 10px;
  color: #91a082;
  line-height: 1.8;
}
@media (max-width: 600px) {
  .servo-summary {
    gap: 18px;
  }
  .servo-summary p {
    margin-left: 0;
    max-width: none;
    width: 100%;
  }
}
@media (max-width: 1100px) {
  .servo-compact .servo-table th,
  .servo-compact .servo-table td { padding-left: 2px; padding-right: 2px; }
  .servo-compact .servo-table thead th:last-child { width: 16%; }
  .servo-compact .servo-unavailable { padding: 2px 1px; }
}
</style>

<style scoped>
.servo-compact {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.servo-compact > .panel-title {
  height: 43px;
  min-height: 0;
  padding: 0 12px;
  flex-shrink: 0;
}
.servo-compact .servo-summary {
  padding: 10px 12px;
  gap: 12px;
  justify-content: space-between;
  flex-shrink: 0;
  flex-wrap: nowrap;
}
.servo-compact .servo-summary > div {
  min-width: 0;
}
.servo-compact .servo-summary span {
  font-size: 9px;
}
.servo-compact .servo-summary strong {
  font-size: 15px;
  margin-top: 4px;
}
.servo-compact .servo-summary small {
  font-size: 9px;
}
.servo-compact .servo-summary p {
  display: none;
}
.servo-compact .servo-table-scroll {
  flex: 1;
  min-height: 0;
  overflow: visible;
}
.servo-compact .servo-table {
  height: 100%;
  min-width: 0;
  table-layout: fixed;
  font-size: 10px;
  white-space: normal;
}
.servo-compact .servo-table caption {
  display: none;
}
.servo-compact .servo-table th,
.servo-compact .servo-table td {
  padding: 0 5px;
  line-height: 1.2;
  vertical-align: middle;
  text-align: center;
}
.servo-compact .servo-table th:first-child {
  padding-left: 10px;
  text-align: left;
  width: 28%;
}
.servo-compact .servo-table th:last-child {
  width: 13%;
}
.servo-compact .servo-table thead {
  height: 37px;
}
.servo-compact .servo-table thead th {
  font-size: 9px;
}
.servo-compact .servo-table thead small {
  font-size: 8px;
  margin-top: 2px;
}
.servo-compact .servo-group {
  height: 22px;
}
.servo-compact .servo-group th {
  font-size: 9px;
}
.servo-compact .servo-group span {
  font-size: 8px;
  margin-left: 7px;
}
.servo-compact .servo-id {
  width: 16px;
  margin-right: 4px;
  font-size: 9px;
}
.servo-compact .servo-unavailable {
  font-size: 8px;
  padding: 2px 4px;
  white-space: nowrap;
}
.servo-compact .servo-footnote {
  font-size: 8px;
  line-height: 1.6;
  padding: 8px 12px;
  flex-shrink: 0;
  border-top: 1px solid #e9eee4;
}
@media (max-height: 720px) {
  .servo-compact > .panel-title {
    height: 36px;
  }
  .servo-compact .servo-summary {
    padding: 7px 10px;
  }
  .servo-compact .servo-group {
    height: 19px;
  }
  .servo-compact .servo-table thead {
    height: 31px;
  }
  .servo-compact .servo-footnote {
    padding: 5px 10px;
  }
  .servo-compact .servo-table {
    font-size: 9px;
  }
}
</style>
