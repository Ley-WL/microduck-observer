<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useTelemetry } from "../store";
const state = useTelemetry();
const action = ref<"poweroff" | "reboot" | null>(null);
const busy = ref(false), message = ref(""), failure = ref("");
const label = computed(() => action.value === "poweroff" ? "关机" : "重启");
const available = computed(() => state.info?.source === "hardware" && state.connection === "在线");
watch(() => state.info?.bootId, () => { busy.value = false; message.value = ""; });
async function submit() {
  if (!action.value || busy.value || !available.value) return;
  const requested = action.value, endpoint = state.endpoint.replace(/\/$/, "");
  const bootId = state.info.bootId;
  busy.value = true; failure.value = "";
  try {
    const response = await fetch(`${endpoint}/api/v1/system/${requested}`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ confirm: true, bootId }), signal: AbortSignal.timeout(5000),
    });
    const result = await response.json();
    if (!response.ok || !result.accepted) throw new Error(result.detail || "请求未确认");
    action.value = null;
    message.value = requested === "poweroff"
      ? "主板正在关机。等待约30秒，并确认关机完成后再断电；舵机电源仍需单独关闭。"
      : "主板正在重启，页面会自动重新连接。请勿断电。";
  } catch (e) {
    busy.value = false;
    failure.value = `${String(e)}。请检查主板状态，勿直接断电。`;
  }
}
</script>
<template>
  <div v-if="state.info?.source === 'hardware'" class="power-controls">
    <button :disabled="!available || busy" @click="action = 'reboot'; failure = ''">重启</button>
    <button :disabled="!available || busy" @click="action = 'poweroff'; failure = ''">关机</button>
    <div v-if="message" class="power-notice" role="status">{{ message }}</div>
    <div v-if="action" class="power-overlay" @click.self="!busy && (action = null)">
      <section class="power-dialog" role="dialog" aria-modal="true" :aria-label="`确认${label}`">
        <h2>确认{{ label }}主板？</h2>
        <p>请先托稳机器人，停止操作。{{ label }}会中断平台连接；舵机电源不会随主板关闭。</p>
        <p>{{ action === 'poweroff' ? '系统会正常关闭并保存数据。等待约30秒，并确认关机完成后再拔电。' : '系统会正常重启，完成后页面自动重新连接。' }}</p>
        <p v-if="failure" role="alert">{{ failure }}</p>
        <div class="power-actions"><button :disabled="busy" @click="action = null">取消</button><button :disabled="busy || !available" @click="submit">{{ busy ? '正在提交…' : `确认${label}` }}</button></div>
      </section>
    </div>
  </div>
</template>
<style scoped>
.power-controls,.power-actions{display:flex;gap:8px}.power-controls button{padding:6px 12px;border:1px solid #d4dce6;border-radius:8px;background:white;color:#334155;cursor:pointer}.power-controls button:disabled{opacity:.5;cursor:default}.power-overlay{position:fixed;inset:0;background:#0f172a88;display:grid;place-items:center;z-index:1000}.power-dialog{width:min(440px,calc(100vw - 40px));padding:24px;border-radius:16px;background:white;color:#1e293b;box-shadow:0 20px 80px #0004}.power-dialog p{line-height:1.7;font-size:14px}.power-actions{justify-content:flex-end}.power-actions button:last-child{background:#b91c1c;color:white;border-color:#b91c1c}.power-notice{position:fixed;top:72px;right:20px;max-width:380px;padding:16px;background:#fff7ed;border:1px solid #fed7aa;border-radius:12px;z-index:999;font-size:14px;line-height:1.6}
</style>
