<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue';
import { useTelemetry } from '../store';
const props = defineProps<{ compact?: boolean }>();
const crosshair = ref(false);
const telemetry = useTelemetry();
const video = ref<HTMLVideoElement>();
const status = ref('未连接'), error = ref(''), resolution = ref('—'), fps = ref('—'), bitrate = ref('—');
const running = ref(false);
const live = computed(() => status.value === '实时画面');
let socket: WebSocket | undefined, peer: RTCPeerConnection | undefined;
let sessionId = '', generation = 0, starting = false;
let timer: ReturnType<typeof setInterval> | undefined, deadline: ReturnType<typeof setTimeout> | undefined;
let lastFrames = 0, lastBytes = 0, lastTimestamp = 0, lastFrameAt = 0;
function stop() {
  generation++;
  clearInterval(timer); clearTimeout(deadline);
  if (socket?.readyState === WebSocket.OPEN && sessionId) socket.send(JSON.stringify({type:'endSession',sessionId}));
  socket?.close(); peer?.close(); socket=undefined; peer=undefined; sessionId=''; starting=false;
  if (video.value) video.value.srcObject=null;
  running.value=false; status.value='已停止'; resolution.value=fps.value=bitrate.value='—';
  lastFrames=lastBytes=lastTimestamp=lastFrameAt=0;
}
function fail(message: string) { stop(); status.value='连接中断'; error.value=message; }
async function connect() {
  stop(); error.value=''; running.value=true; status.value='连接媒体服务';
  const current=generation;
  const active=()=>generation===current;
  try {
    const url = new URL(telemetry.endpoint);
    url.protocol=url.protocol==='https:'?'wss:':'ws:'; url.port='8443'; url.pathname='/'; url.search=''; url.hash='';
    const ws = socket = new WebSocket(url);
    const send = (message: object) => { if(active() && ws.readyState===WebSocket.OPEN) ws.send(JSON.stringify(message)); };
    deadline=setTimeout(()=>{if(active()) fail('媒体连接超时，请检查摄像头服务后重连。');},20000);
    ws.onerror=()=>{if(active()) fail('无法连接主控媒体服务（端口 8443）。');};
    ws.onclose=()=>{if(active()) fail('媒体服务连接已关闭，请重连。');};
    let queue=Promise.resolve();
    ws.onmessage=(event)=>{
      queue=queue.then(async()=>{
        if(!active()) return;
        const msg=JSON.parse(event.data);
        if(msg.type==='welcome') send({type:'list'});
        if(msg.type==='list' && !starting && !sessionId) {
          if(!msg.producers?.length) { fail('未发现视频源，请检查 mediad 服务。'); return; }
          starting=true; send({type:'startSession',peerId:msg.producers[0].id});
        }
        if(msg.type==='sessionStarted') {
          sessionId=msg.sessionId; status.value='协商视频连接';
          const pc=peer=new RTCPeerConnection({iceServers:[]});
          pc.ontrack=(event)=>{
            if(!active() || event.track.kind!=='video' || !video.value) return;
            video.value.srcObject=new MediaStream([event.track]);
            void video.value.play().catch(()=>{if(active()) error.value='播放被浏览器暂停，请点击视频播放。';});
          };
          // This observer never sends robot control RPCs over the offered data channel.
          pc.ondatachannel=event=>{event.channel.onmessage=()=>{};};
          pc.onicecandidate=event=>{if(event.candidate) send({type:'peer',sessionId,ice:{candidate:event.candidate.candidate,sdpMLineIndex:event.candidate.sdpMLineIndex}});};
          pc.onconnectionstatechange=()=>{
            if(!active()) return;
            if(pc.connectionState==='failed') fail('视频链路失败，请重连。');
            else if(pc.connectionState==='disconnected') status.value='连接暂时中断';
          };
          timer=setInterval(async()=>{
            try {
              const stats=await pc.getStats(); if(!active()) return;
              stats.forEach(s=>{
                if(s.type!=='inbound-rtp' || s.kind!=='video') return;
                const frames=s.framesDecoded??0, bytes=s.bytesReceived??0;
                const elapsed=(s.timestamp-lastTimestamp)/1000;
                if(lastTimestamp && elapsed>0) {
                  fps.value=((frames-lastFrames)/elapsed).toFixed(1);
                  bitrate.value=((bytes-lastBytes)*8/elapsed/1000000).toFixed(2);
                }
                if(frames>lastFrames) {
                  lastFrameAt=Date.now(); clearTimeout(deadline); status.value='实时画面';
                } else if(lastFrameAt && Date.now()-lastFrameAt>3000) status.value='画面停滞';
                lastFrames=frames; lastBytes=bytes; lastTimestamp=s.timestamp;
                if(video.value?.videoWidth) resolution.value=`${video.value.videoWidth} × ${video.value.videoHeight}`;
              });
            } catch { /* Closed peers can finish a pending statistics request. */ }
          },1000);
        }
        if(msg.type==='peer' && peer && msg.sessionId===sessionId) {
          const pc=peer;
          if(msg.sdp) {
            await pc.setRemoteDescription(msg.sdp); if(!active()) return;
            const answer=await pc.createAnswer(); if(!active()) return;
            await pc.setLocalDescription(answer); if(!active()) return;
            send({type:'peer',sessionId,sdp:{type:'answer',sdp:answer.sdp}});
          } else if(msg.ice) await pc.addIceCandidate(msg.ice);
        }
        if(['error','endSession','sessionRejected'].includes(msg.type)) fail('媒体会话结束或被拒绝，请重连。');
      }).catch((e)=>{if(active()) fail(`视频协商失败：${e instanceof Error?e.message:String(e)}`);});
    };
  } catch(e) { if(active()) fail(String(e)); }
}
function fullscreen() { void video.value?.requestFullscreen().catch(()=>{error.value='当前浏览器不支持全屏播放。';}); }
watch(()=>telemetry.endpoint,()=>void connect());
onMounted(()=>void connect());
onBeforeUnmount(stop);
</script>

<template>
  <section class="camera-page" :class="{ compact: props.compact }" aria-label="摄像头实时画面">
    <div v-if="compact" class="sensor-title-row">
      <div class="sensor-identity"><span class="sensor-icon">▣</span><div><h2>IMX219 摄像头</h2><small>RGB CAMERA</small></div></div>
      <span class="live-badge" :class="{ live }">{{ status }}</span>
    </div>
    <header v-else class="camera-heading"><div><div class="eyebrow">LIVE CAMERA</div><h1>IMX219 摄像头</h1><p>机器人视角 · 实时视频</p></div><span class="camera-status" :class="{ live }">● {{ status }}</span></header>
    <div v-if="compact" class="stream-meta"><span>WebRTC · 机器人视角</span><span>{{ fps }} fps</span></div>
    <div class="camera-actions"><button class="button" @click="connect">重连</button><button class="button" :disabled="!running" @click="stop">停止</button><button class="button" :class="{ selected: crosshair }" @click="crosshair = !crosshair">{{ crosshair ? "关闭准星" : "中心准星" }}</button><button class="button" @click="fullscreen">全屏 ↗</button></div>
    <div class="camera-stage">
      <video ref="video" autoplay muted playsinline controls aria-label="机器人实时视频" />
      <div v-if="crosshair" class="camera-crosshair"></div>
      <div v-if="!live" class="camera-overlay"><span class="camera-empty-icon">▣</span><strong>{{ status }}</strong><span>{{ error || '等待实时视频帧' }}</span></div>
    </div>
    <div class="camera-metrics"><span>分辨率 <b>{{ resolution }}</b></span><span>接收帧率 <b>{{ fps }} fps</b></span><span>接收码率 <b>{{ bitrate }} Mbps</b></span><span>WebRTC · 局域网实时视频</span></div>
    <p class="camera-note">当前使用已实测的固定曝光与白平衡，光线变化时画面亮度和色彩可能变化。离开本页自动停止观看。</p>
  </section>
</template>

<style scoped>
.camera-page{height:calc(100dvh - 125px);min-height:400px;display:flex;flex-direction:column;gap:16px}
.camera-heading{height:auto;padding:0;background:transparent;border:0;flex-shrink:0;display:flex;align-items:center;justify-content:space-between;gap:12px}.camera-heading h1{margin:5px 0;font-size:30px}.camera-heading p,.camera-note{color:var(--muted,#6b7972);margin:0;font-size:12px}.camera-actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}.camera-status{background:#fff0db;color:#96611c;border-radius:20px;padding:8px 12px;font-size:12px}.camera-status.live{background:#e1f3e7;color:#267448}.camera-stage{position:relative;flex:1;min-height:0;background:#101b17;border-radius:16px;overflow:hidden;display:flex;align-items:center;justify-content:center}.camera-stage video{width:100%;height:100%;object-fit:contain}.camera-overlay{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:12px;background:#101b17d9;color:#e3eee8;pointer-events:none}.camera-overlay span{font-size:13px}.camera-metrics{display:flex;gap:24px;flex-wrap:wrap;color:#6b7972;font-size:12px}.camera-metrics b{color:#284b3a;font-variant-numeric:tabular-nums;margin-left:8px}@media(max-width:800px){.camera-heading{align-items:flex-start;flex-direction:column}.camera-metrics{gap:10px}.camera-page{height:calc(100dvh - 110px)}}
.camera-crosshair{position:absolute;left:50%;top:50%;width:28px;height:28px;transform:translate(-50%,-50%);pointer-events:none;filter:drop-shadow(0 1px 2px #000)}
.camera-crosshair:before,.camera-crosshair:after{content:"";position:absolute;background:#53ffdb}.camera-crosshair:before{width:28px;height:1px;top:14px}.camera-crosshair:after{height:28px;width:1px;left:14px}
.camera-page.compact{height:100%;min-height:0;gap:10px;padding:14px;background:white;border:1px solid #dfe7e2;border-radius:14px;overflow:hidden}
.camera-page.compact .camera-heading{height:auto;min-height:0;padding:0;display:flex;flex-direction:column;align-items:stretch}
.compact .camera-heading h1{font-size:18px;margin:3px 0}.compact .eyebrow,.compact .camera-heading p{display:none}
.compact .camera-actions{gap:5px}.compact .camera-actions .button{font-size:11px;padding:6px 8px;min-height:28px}.compact .camera-status{font-size:11px;padding:6px 8px}
.compact .camera-stage{border-radius:10px}.compact .camera-metrics{gap:8px;display:grid;grid-template-columns:1fr 1fr;font-size:11px}.compact .camera-metrics b{margin-left:4px}.compact .camera-metrics>span:last-child{grid-column:1/-1}
.compact .camera-note{font-size:10px;line-height:1.5}.compact .camera-overlay{padding:14px;text-align:center}.compact .camera-overlay span{font-size:12px}
.compact .camera-actions{height:30px;flex-shrink:0;flex-wrap:nowrap;gap:6px}.compact .camera-actions .button{flex:1;min-width:0;border-color:#e0e7e6;background:#f7f9f8;color:#54716b;border-radius:7px;font-size:11px;padding:5px}.compact .camera-actions .button.selected{background:#e7f3ef;color:#277760;border-color:#a9cfc1}
.camera-page.compact{padding:16px;gap:12px;border-color:#e0e8e5;border-radius:18px;box-shadow:0 4px 22px #18372b05}
.compact .camera-stage{background:#172522;border:1px solid #263b35;border-radius:12px}.compact .camera-overlay{background:radial-gradient(ellipse at 50% 40%,#263b35,#13211e);gap:10px;color:#cbdad4}.camera-empty-icon{font-size:32px!important;line-height:64px;width:64px;height:64px;border-radius:18px;background:#ffffff08;border:1px solid #ffffff12;color:#8ea89c}.compact .camera-overlay strong{font-size:15px;font-weight:500}.compact .camera-overlay span:not(.camera-empty-icon){color:#8ca498;max-width:260px;line-height:1.7}
.compact .camera-metrics{padding:12px;background:#f6f8f7;border:1px solid #edf1ef;border-radius:10px;gap:10px}.compact .camera-metrics b{font-weight:600}.compact .camera-note{padding:2px 2px 0;color:#84958b}
@media(max-height:760px){.camera-page.compact{gap:8px;padding:12px}.compact .camera-metrics{padding:9px}}
</style>
