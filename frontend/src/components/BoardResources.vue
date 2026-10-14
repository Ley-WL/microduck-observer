<script setup lang="ts">
import { computed } from "vue";
import { useTelemetry } from "../store";
const state = useTelemetry();
const data = computed(() => state.system?.data.resources);
const numeric = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const percent = (v: unknown) => numeric(v) ? `${v.toFixed(1)}%` : "—";
const temperature = computed(() => numeric(data.value?.temperature?.celsius) ? `${data.value.temperature.celsius.toFixed(1)}°C` : "—");
const cpuFrequency = computed(() => {
  const cores = data.value?.cpuFrequency?.cores;
  if (!Array.isArray(cores) || !cores.length) return "—";
  return cores.map((core: any) => numeric(core?.mhz) ? `${core.mhz.toFixed(0)}` : "—").join(" / ") + " MHz";
});
const cpuFrequencyTitle = computed(() => {
  const frequency = data.value?.cpuFrequency;
  const governor = typeof frequency?.governor === "string" ? frequency.governor : "未知";
  const cores = Array.isArray(frequency?.cores) ? frequency.cores : [];
  const max = cores.find((core: any) => numeric(core?.maxMHz))?.maxMHz;
  return `各核当前频率；策略 ${governor}${numeric(max) ? `；系统上限 ${max.toFixed(0)} MHz` : ""}`;
});
const memory = computed(() => {
  const m = data.value?.memory;
  if (!numeric(m?.usedBytes) || !numeric(m?.totalBytes)) return "—";
  return `${(m.usedBytes / 1048576).toFixed(0)} / ${(m.totalBytes / 1048576).toFixed(0)} MiB`;
});
</script>
<template>
  <section v-if="state.info?.source === 'hardware'" class="board-resources" aria-label="主板运行状态">
    <span v-if="state.connection !== '在线'" class="resource-heading">已断开 · 最后读数</span>
    <span>温度 <strong>{{ temperature }}</strong></span>
    <span title="所有CPU核心的平均利用率">CPU <strong>{{ percent(data?.cpuUsagePercent) }}</strong></span>
    <span :title="cpuFrequencyTitle">频率 <strong>{{ cpuFrequency }}</strong></span>
    <span title="已用内存按总内存减可用内存计算，可回收缓存计入可用内存">内存 <strong>{{ memory }}</strong><small>{{ percent(data?.memory?.usagePercent) }}</small></span>
  </section>
</template>
<style scoped>
.board-resources { display:flex; align-items:center; justify-content:center; flex-wrap:wrap; gap:4px 18px; padding:0 12px; margin:0; flex:1; min-width:0; font-size:12px; color:#5d6c7e; }
.board-resources strong { color:#182b3f; font-variant-numeric:tabular-nums; margin-left:6px; }
.board-resources small { font-size:11px; color:#7d8997; margin-left:8px; }
.resource-heading { font-weight:600; }
@media (max-width:1100px) { .board-resources { gap:3px 10px; font-size:11px; } .board-resources small { margin-left:4px; } }
</style>
