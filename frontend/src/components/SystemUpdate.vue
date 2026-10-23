<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue';
const props = defineProps<{ endpoint: string; online: boolean }>();
const info = ref<any>(), preview = ref<any>(), file = ref<File>();
const busy = ref(false), message = ref(''), progress = ref(0), accepted = ref(false);
let generation = 0, xhr: XMLHttpRequest | undefined;
const base = () => props.endpoint.replace(/\/$/, '') + '/api/v1/update';
async function refresh() {
  const current = generation;
  try {
    const response = await fetch(base(), { signal: AbortSignal.timeout(9000) });
    if (!response.ok) throw new Error('此版本尚未安装网页更新接口');
    const value = await response.json();
    if (current === generation) {
      info.value = value;
      if (accepted.value && ['success', 'failed'].includes(value.status?.phase)) {
        accepted.value = false; busy.value = false;
        message.value = value.status.phase === 'success' ? '更新成功。请刷新页面载入新版界面。' : value.status.message || '更新失败，请检查状态';
      }
    }
  } catch (e) {
    if (current === generation && !accepted.value) { info.value = undefined; message.value = String(e); }
  }
}
watch(() => props.endpoint, () => { generation++; xhr?.abort(); info.value = undefined; preview.value = undefined; accepted.value = false; busy.value = false; void refresh(); }, { immediate: true });
const timer = setInterval(() => { if (!busy.value || accepted.value) void refresh(); }, 3000);
onUnmounted(() => { generation++; clearInterval(timer); xhr?.abort(); });
function select(event: Event) { file.value = (event.target as HTMLInputElement).files?.[0]; preview.value = undefined; message.value = ''; }
async function upload() {
  if (!file.value || busy.value) return;
  if (!file.value.size || file.value.size > 128 * 1024 * 1024) { message.value = '请选择不超过128MiB的更新包'; return; }
  const current = generation; busy.value = true; progress.value = 0; message.value = ''; preview.value = undefined;
  try {
    const value = await new Promise<any>((resolve, reject) => {
      const request = new XMLHttpRequest(); xhr = request;
      request.open('POST', base() + '/upload'); request.timeout = 260000;
      request.setRequestHeader('Content-Type', 'application/gzip'); request.setRequestHeader('X-Microduck-Update', '1');
      request.upload.onprogress = e => { if (current === generation && e.lengthComputable) progress.value = Math.round(e.loaded / e.total * 100); };
      request.onload = () => { try { const v = JSON.parse(request.responseText); request.status >= 200 && request.status < 300 ? resolve(v) : reject(new Error(v.detail || '校验失败')); } catch (e) { reject(e); } };
      request.onerror = () => reject(new Error('上传连接中断')); request.ontimeout = () => reject(new Error('上传超时')); request.onabort = () => reject(new Error('已取消上传'));
      request.send(file.value);
    });
    if (current === generation) { preview.value = value; message.value = '签名和文件校验通过，请核对版本后安装。'; }
  } catch (e) { if (current === generation) message.value = String(e); }
  finally { if (current === generation) busy.value = false; }
}
async function install() {
  if (!preview.value || busy.value) return;
  const current = generation; busy.value = true; message.value = '正在提交安装请求…';
  try {
    const response = await fetch(base() + '/install', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: preview.value.id, bootId: info.value.bootId, confirm: true }), signal: AbortSignal.timeout(15000) });
    const value = await response.json(); if (!response.ok) throw new Error(value.detail || '安装请求失败');
    if (current === generation) { accepted.value = true; preview.value = undefined; message.value = '正在更新，服务会短暂断开；请保持主板供电，页面会自动查询结果。'; }
  } catch (e) { if (current === generation) { busy.value = false; message.value = String(e) + '；如服务已断开，请等待重连后核对更新状态，勿重复安装。'; } }
}
function reload() { window.location.reload(); }
</script>
<template>
  <section class="update-panel">
    <h2>系统更新</h2>
    <p>上传签名更新包，可更新后端程序、网页界面和模型，支持单独或组合更新。包内模型按同名文件更新，未包含的模型保留；标定和设备设置保留。</p>
    <p>当前版本：<strong>{{ info?.current?.release || info?.current?.version || '等待连接' }}</strong></p>
    <p v-if="!info?.available" class="notice">{{ info?.message || '旧版本需先安装一次新版 Observer 和更新组件，才能使用此功能。' }}</p>
    <label>选择更新包（.tar.gz，最多128MiB）<input type="file" accept=".gz,.tar.gz" :disabled="busy || !info?.available" @change="select" /></label>
    <button :disabled="!file || busy || !online || !info?.available" @click="upload">上传并校验</button>
    <p v-if="busy && !accepted">{{ progress < 100 ? `上传进度 ${progress}%` : '正在校验签名和文件…' }}</p>
    <div v-if="preview" class="preview">
      <p>待安装版本：<strong>{{ preview.version }}</strong>（{{ preview.versionCode }}）</p>
      <p>包含：{{ preview.components.map((c: string) => c === 'backend' ? '后端程序' : c === 'models' ? '模型文件' : '网页界面').join('、') }} · {{ preview.files }} 个文件</p>
      <p v-if="preview.models?.length">更新模型：{{ preview.models.join('、') }}（同名覆盖，失败随服务回退）</p>
      <p>请先停止动作并全部卸力。安装会短暂重启服务，启动验证失败时自动恢复旧版。</p>
      <button :disabled="busy || !online" @click="install">确认安装此版本</button>
    </div>
    <p role="status" aria-live="polite">{{ message }}</p>
    <p v-if="info?.status?.phase && info.status.phase !== 'idle'">主板更新状态：{{ ({queued:'等待安装',downloading:'准备文件',installing:'安装与验证',recovering:'恢复旧版本',success:'成功',failed:'失败'} as Record<string,string>)[info.status.phase] || info.status.phase }} {{ info.status.message || '' }}</p>
    <button v-if="info?.status?.phase === 'success'" @click="reload">刷新到新版界面</button>
  </section>
</template>
<style scoped>
.update-panel{max-width:850px;margin:24px auto;padding:28px;background:#fff;border:1px solid #dce3e7;border-radius:18px;color:#263641;line-height:1.7}.update-panel h2{margin-top:0}.update-panel label{display:block;margin:24px 0 14px}.update-panel input{display:block;margin-top:8px;max-width:100%}.update-panel button{padding:10px 18px;border:1px solid #b6c9cb;border-radius:9px;background:#e6f4ee;color:#244a3d;cursor:pointer}.update-panel button:disabled{opacity:.5;cursor:not-allowed}.notice{color:#8a5314}.preview{margin:22px 0;padding:18px;background:#f1f6f6;border-radius:12px}
</style>
