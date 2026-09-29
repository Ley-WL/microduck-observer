<script setup lang="ts">
import { computed } from 'vue';
import { tofColumns, type DepthCell } from '../tof';
const props = defineProps<{ cells: DepthCell[]; range: number; selected: number; live: boolean }>();
const emit = defineEmits<{ select: [index: number] }>();
const columns = computed(() => tofColumns(props.cells));
const points = computed(() => columns.value.filter(p => p !== null));
const nearest = computed(() => [...points.value].sort((a,b) => a.distance - b.distance)[0]);
// Display fan is schematic: column positions are not calibrated bearing angles.
function xy(distance: number, column: number) {
  const angle = (-63 + column * 18) * Math.PI / 180;
  const radius = Math.min(distance / props.range, 1) * 160;
  return { x: 180 + Math.sin(angle)*radius, y: 218 - Math.cos(angle)*radius };
}
function arc(distance: number) {
  const r = distance / props.range * 160;
  const x = Math.sin(72*Math.PI/180)*r, y = 218-Math.cos(72*Math.PI/180)*r;
  return `M ${180-x} ${y} A ${r} ${r} 0 0 1 ${180+x} ${y}`;
}
const rings = computed(() => [...new Set([200,500,1000,2000,4000,props.range])].filter(d => d <= props.range).sort((a,b)=>a-b));
const color = (d: number) => d <= 200 ? '#ff7862' : d <= 500 ? '#ffc56b' : d <= 1000 ? '#70dba9' : '#72b9f5';
</script>
<template>
  <div class="tof-radar">
    <div class="radar-summary"><span>{{ live ? '最近有效回波' : '历史回波 / 等待数据' }}</span><strong>{{ nearest ? (nearest.distance/10).toFixed(1)+' cm' : '—' }}</strong></div>
    <svg viewBox="0 0 360 258" aria-label="ToF 前方扇环距离示意图">
      <path :d="`${arc(range)} L 180 218 Z`" fill="#203d35" fill-opacity=".6" />
      <g v-for="r in rings" :key="r"><path :d="arc(r)" fill="none" stroke="#426556" stroke-width=".8" /><text x="180" :y="218-r/range*160-5" text-anchor="middle" fill="#88a698" font-size="9">{{ r/10 }} cm</text></g>
      <path v-for="(_,i) in columns" :key="'ray'+i" :d="`M 180 218 L ${xy(range,i).x} ${xy(range,i).y}`" stroke="#365449" stroke-dasharray="2 5" stroke-width=".7" />
      <g v-for="p in points" :key="p.column" role="button" tabindex="0" :aria-label="`第${p.column+1}列最近距离${(p.distance/10).toFixed(1)}厘米${p.distance>range?'，超出显示范围':''}`" @click="emit('select',p.index)" @keydown.enter="emit('select',p.index)" @keydown.space.prevent="emit('select',p.index)" class="radar-point">
        <title>第{{ Math.floor(p.index/8)+1 }}行 · 第{{ p.column+1 }}列 · {{ (p.distance/10).toFixed(1) }} cm{{ p.distance > range ? '，超出显示范围' : '' }}</title>
        <line x1="180" y1="218" :x2="xy(p.distance,p.column).x" :y2="xy(p.distance,p.column).y" :stroke="color(p.distance)" :opacity="nearest?.index===p.index ? .6 : .16" />
        <circle :cx="xy(p.distance,p.column).x" :cy="xy(p.distance,p.column).y" r="13" fill="transparent" />
        <circle v-if="nearest?.index===p.index || selected===p.index" :cx="xy(p.distance,p.column).x" :cy="xy(p.distance,p.column).y" :r="nearest?.index===p.index ? 10 : 8" fill="none" :stroke="nearest?.index===p.index ? '#ffc56b' : '#fff'" stroke-width="1.5" />
        <circle :cx="xy(p.distance,p.column).x" :cy="xy(p.distance,p.column).y" :r="nearest?.index===p.index ? 4.5 : 3" :fill="p.distance > range ? '#162a23' : color(p.distance)" :stroke="color(p.distance)" stroke-width="1.5" />
        <text v-if="p.distance>range" :x="xy(p.distance,p.column).x" :y="xy(p.distance,p.column).y-14" fill="#99c9ee" font-size="10" text-anchor="middle">↑</text>
      </g>
      <circle cx="180" cy="218" r="4" fill="#bed6ca" /><text x="180" y="242" text-anchor="middle" fill="#92b2a0" font-size="10">ToF · 每列最近有效距离</text>
      <text x="18" y="228" fill="#6d907d" font-size="9">列 1</text><text x="320" y="228" fill="#6d907d" font-size="9">列 8</text>
    </svg>
    <button v-if="nearest" class="nearest-button" @click="emit('select',nearest.index)">◎ 定位最近回波 · {{ Math.floor(nearest.index/8)+1 }}行 {{ nearest.column+1 }}列</button>
    <div v-else class="radar-empty">暂无有效回波</div>
    <div class="radar-note">扇角为列顺序示意，非实际方位；空心 ↑ 为超出显示范围。</div>
  </div>
</template>
<style scoped>
.tof-radar{width:100%;height:100%;min-height:0;display:flex;flex-direction:column;background:radial-gradient(ellipse at 50% 90%,#203f32,#15251f);border-radius:10px;padding:12px;color:#c0d8c9;overflow:hidden}
.radar-summary{display:flex;justify-content:space-between;align-items:center;gap:8px;font-size:10px;flex-shrink:0}.radar-summary strong{font-size:22px;font-weight:550;color:#ffd294;font-variant-numeric:tabular-nums}
svg{display:block;width:100%;flex:1;min-height:80px;overflow:visible}.radar-point{cursor:pointer;outline:none}.radar-point:focus circle{stroke:white;stroke-width:2}
.nearest-button{background:#304739!important;color:#efcc94!important;border:1px solid #52654b!important;border-radius:6px!important;font-size:10px!important;padding:6px!important;flex-shrink:0}.radar-empty{font-size:11px;text-align:center;padding:5px}.radar-note{font-size:9px;line-height:1.5;color:#819e8d;margin-top:7px;flex-shrink:0}
</style>
