<script setup lang="ts">
import { computed, ref } from 'vue';
import { useTelemetry } from '../store';
import TofDepthMap from './TofDepthMap.vue';
import { tofStats } from '../tof';
const state = useTelemetry();
const selected = ref(0);
const range = ref(2000);
const smooth = ref(true);
const frame = computed(() => state.tof?.data);
const stats = computed(() => tofStats(frame.value as any));
const live = computed(() => state.connection === '在线' && state.tof?.valid && state.tofAge <= 1500 && state.system?.data.tof?.state !== 'offline');
const label = computed(() => !state.info?.capabilities?.tof ? '未启用' : state.connection !== '在线' ? '连接断开' : state.system?.data.tof?.state === 'offline' ? '传感器离线' : !frame.value ? '等待数据' : !live.value ? '数据过期' : '在线');
const cm = (value: number | null | undefined) => value == null ? '—' : (value / 10).toFixed(1);
const cells = computed(() => Array.from({length: 64}, (_, i) => ({
  distance: frame.value?.distanceMm[i], status: frame.value?.status[i],
  valid: frame.value?.status[i] === 5,
})));
</script>

<template>
  <section class="panel tof-panel">
    <div class="panel-title">
      <div><h2>ToF 深度图</h2><span class="tof-status" :class="{ online: live }">{{ label }}</span><span v-if="state.paused" class="subtle-tag">显示已暂停</span></div>
      <span class="muted">VL53L5CX · 8 × 8 · {{ state.tof?.source === 'hardware' ? '实机数据' : '等待硬件' }}</span>
    </div>
    <div class="tof-layout">
      <div class="tof-map">
        <div class="tof-map-caption"><span>原始分辨率 8 × 8 · 近亮远暗</span><label><input v-model="smooth" type="checkbox" /> 平滑显示</label></div>
        <TofDepthMap :cells="cells" :range="range" :smooth="smooth" :selected="selected"
          :class="{ stale: !live || state.paused }" @select="selected = $event" />
        <div class="tof-scale"><span>近 0 cm</span><i></i><span>远 {{ range / 10 }} cm</span></div>
        <p class="muted">紫色斜纹表示无效测量。点击图中任意位置查看原始距离；平滑仅用于显示，不增加测量分辨率。</p>
      </div>
      <div class="tof-detail">
        <div class="tof-readings">
          <article><span>有效区域 · status 5</span><strong>{{ stats.count }} <small>/ 64</small></strong></article>
          <article><span>实际采集频率</span><strong>{{ live ? frame?.hz.toFixed(1) : '—' }} <small>Hz</small></strong></article>
          <article><span>最近有效距离</span><strong>{{ cm(stats.min) }} <small>cm</small></strong></article>
          <article><span>有效区域中位数</span><strong>{{ cm(stats.median) }} <small>cm</small></strong></article>
        </div>
        <div class="tof-inspector">
          <h3>分区 {{ Math.floor(selected / 8) + 1 }} 行 · {{ selected % 8 + 1 }} 列</h3>
          <p>距离：<b>{{ cells[selected].valid ? cm(cells[selected].distance) + ' cm' : '无有效距离' }}</b></p>
          <p>原始值：{{ cells[selected].distance ?? '—' }} mm · 状态 {{ cells[selected].status ?? '—' }}</p>
          <p>帧序号 {{ frame?.sensorSeq ?? '—' }} · 年龄 {{ Number.isFinite(state.tofAge) ? Math.round(state.tofAge) + ' ms' : '—' }}</p>
          <label>深度显示上限 <select v-model.number="range"><option :value="1000">100 cm</option><option :value="2000">200 cm</option><option :value="4000">400 cm</option></select></label>
        </div>
        <div v-if="!live" class="notice warning">{{ frame ? '当前保留最后一帧，不代表实时距离。' : '等待主控 tofd 数据流。' }} {{ state.system?.data.tof?.error }}</div>
        <p class="muted">仅将 status=5 计入有效距离。安装方向尚未标定，深度图不映射机器人的左右或上下；统计值来自整个视场，不代表单一目标。</p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.tof-panel { margin-bottom: 20px; }
.tof-status { padding: 4px 10px; border-radius: 20px; background: #fce8e5; color: #af3429; font-size: 12px; }
.tof-status.online { background: #e0f4eb; color: #19714d; }
.tof-layout { display: grid; grid-template-columns: minmax(320px, 1.25fr) minmax(260px, 1fr); gap: 28px; padding: 24px; }
.tof-map { max-width: 600px; width: 100%; margin: auto; }
.tof-map-caption,.tof-scale { display: flex; justify-content: space-between; gap: 14px; font-size: 12px; color: #76808c; margin: 0 0 12px; }
.stale { opacity: .5; }
.tof-scale { align-items: center; margin: 16px 0; }
.tof-scale i { flex: 1; height: 8px; border-radius: 5px; background: linear-gradient(90deg,#f5f5f5,#141414); }
.tof-readings { display: grid; grid-template-columns: 1fr 1fr; gap: 20px; }
.tof-readings article { padding: 16px; background: #f5f7fa; border-radius: 10px; }
.tof-readings span { display: block; color: #667283; font-size: 12px; margin-bottom: 12px; }
.tof-readings strong { font-size: 26px; color: #25384c; }
.tof-readings small { font-size: 13px; font-weight: normal; }
.tof-inspector { margin: 22px 0; padding: 18px; border: 1px solid #e1e6ee; border-radius: 10px; font-size: 13px; }
.tof-inspector h3 { margin-top: 0; }.tof-inspector select { margin-left: 10px; padding: 6px; }
.tof-detail > p,.tof-map > p { line-height: 1.7; font-size: 12px; }
@media(max-width: 900px) { .tof-layout { grid-template-columns: 1fr; } }
</style>
