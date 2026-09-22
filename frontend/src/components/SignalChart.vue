<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import * as echarts from "echarts/core";
import { LineChart } from "echarts/charts";
import { GridComponent, TooltipComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
echarts.use([LineChart, GridComponent, TooltipComponent, CanvasRenderer]);
const props = defineProps<{
  samples: { t: number; gyro: number[]; accel: number[] }[];
  kind: "gyro" | "accel";
}>();
const host = ref<HTMLDivElement>();
let chart: echarts.ECharts, observer: ResizeObserver;
function draw() {
  if (!chart) return;
  chart.setOption({
    animation: false,
    grid: { left: 48, right: 16, top: 12, bottom: 25 },
    tooltip: { trigger: "axis" },
    xAxis: {
      type: "value",
      min: props.samples.length ? Math.max(0, props.samples.at(-1)!.t - 60) : 0,
      max: props.samples.at(-1)?.t || 60,
      axisLabel: {
        color: "#87948d",
        formatter: (v: number) => v.toFixed(0) + "s",
      },
      splitLine: { show: false },
      axisLine: { show: false },
      axisTick: { show: false },
    },
    yAxis: {
      type: "value",
      axisLabel: { color: "#87948d", fontSize: 10 },
      splitNumber: 3,
      splitLine: { lineStyle: { color: "#edf1ee" } },
    },
    series: ["X", "Y", "Z"].map((name, i) => ({
      name,
      type: "line",
      showSymbol: false,
      lineStyle: { width: 1.8 },
      itemStyle: { color: ["#37816a", "#dba855", "#7196c2"][i] },
      data: props.samples.map((s) => [s.t, s[props.kind][i]]),
    })),
  });
}
watch(() => [props.samples, props.kind], draw);
onMounted(() => {
  chart = echarts.init(host.value);
  observer = new ResizeObserver(() => chart.resize());
  observer.observe(host.value!);
  draw();
});
onBeforeUnmount(() => {
  observer?.disconnect();
  chart?.dispose();
});
</script>
<template><div ref="host" class="signal-chart"></div></template>
