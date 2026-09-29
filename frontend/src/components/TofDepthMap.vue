<script setup lang="ts">
import { onMounted, ref, watch } from 'vue';
const props = defineProps<{
  cells: { distance: number | undefined; valid: boolean }[];
  range: number;
  smooth: boolean;
  selected: number;
}>();
const emit = defineEmits<{ select: [index: number] }>();
const canvas = ref<HTMLCanvasElement>();
function draw() {
  const ctx = canvas.value?.getContext('2d');
  if (!ctx) return;
  const size = 256, image = ctx.createImageData(size, size);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const cell = props.cells[Math.floor(y / 32) * 8 + Math.floor(x / 32)];
    const offset = (y * size + x) * 4;
    if (!cell?.valid) {
      // Invalid native zones remain masked, never filled by neighbouring depths.
      const stripe = (x + y) % 12 < 3;
      image.data.set(stripe ? [62, 48, 85, 255] : [30, 24, 43, 255], offset);
      continue;
    }
    let distance = cell.distance ?? 0;
    if (props.smooth) {
      const gx = Math.max(0, Math.min(7, (x + .5) / 32 - .5));
      const gy = Math.max(0, Math.min(7, (y + .5) / 32 - .5));
      const x0 = Math.floor(gx), y0 = Math.floor(gy), dx = gx - x0, dy = gy - y0;
      let sum = 0, weight = 0;
      for (let j = 0; j <= 1; j++) for (let i = 0; i <= 1; i++) {
        const neighbour = props.cells[Math.min(7, y0+j)*8 + Math.min(7, x0+i)];
        const w = (i ? dx : 1-dx) * (j ? dy : 1-dy);
        if (neighbour?.valid) { sum += neighbour.distance! * w; weight += w; }
      }
      if (weight > 0) distance = sum / weight;
    }
    const gray = Math.round(245 - 225 * Math.min(1, Math.max(0, distance / props.range)));
    image.data.set([gray, gray, gray, 255], offset);
  }
  ctx.putImageData(image, 0, 0);
}
watch(() => [props.cells, props.range, props.smooth], draw, { flush: 'post' });
onMounted(draw);
</script>
<template>
  <div class="depth-image">
    <canvas ref="canvas" width="256" height="256" aria-label="ToF 灰度深度图，近亮远暗" />
    <div class="depth-points">
      <button v-for="(cell, i) in cells" :key="i" :class="{ selected: selected === i }"
        :aria-label="`第${Math.floor(i/8)+1}行第${i%8+1}列：${cell.valid ? (cell.distance! / 10).toFixed(1)+'厘米' : '无效测量'}`"
        :title="cell.valid ? (cell.distance! / 10).toFixed(1)+' cm' : '无效测量'"
        @click="emit('select', i)"><span v-if="selected === i">+</span></button>
    </div>
  </div>
</template>
<style scoped>
.depth-image { position: relative; aspect-ratio: 1; background: #141414; border-radius: 10px; overflow: hidden; }
canvas { width: 100%; height: 100%; display: block; image-rendering: pixelated; }
.depth-points { position: absolute; inset: 0; display: grid; grid-template-columns: repeat(8,1fr); grid-template-rows: repeat(8,1fr); }
.depth-points button { background: transparent; border: 1px solid transparent; padding: 0; cursor: crosshair; color: #4cf5cf; font-size: 30px; text-shadow: 0 1px 3px black; }
.depth-points button:hover,.depth-points button:focus-visible { border: 1px solid #4cf5cf; outline: none; }
</style>
