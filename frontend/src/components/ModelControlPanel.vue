<script setup lang="ts">
import ModelRuntime from './ModelRuntime.vue';
</script>

<template>
  <section class="model-controls" aria-label="实机模型控制">
    <div class="control-heading">
      <div><span class="control-symbol" aria-hidden="true">◇</span><strong>模型手柄</strong></div>
      <span class="backend-tag">实机模型</span>
    </div>
    <ModelRuntime />
  </section>
</template>

<style scoped>
.model-controls{box-sizing:border-box;min-width:0;width:100%;padding:12px 14px;border:1px solid #c8dace;border-radius:12px;background:linear-gradient(145deg,#f7faf7,#eaf2eb);color:#365a44}
.control-heading,.control-heading>div{display:flex;align-items:center;gap:8px}
.control-heading{justify-content:space-between;padding-bottom:8px;border-bottom:1px solid #d6e3d9}
.control-symbol{color:#668c71;font-size:18px}.control-heading strong{font-size:14px}
.backend-tag{font-size:10px;padding:4px 8px;border-radius:6px;background:#dcebe1;color:#466b52}
</style>
