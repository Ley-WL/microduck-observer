<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue';
import { useTelemetry } from '../store';
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
  <section class="camera-page" aria-label="摄像头实时画面">
    <header class="camera-heading">
      <div><div class="eyebrow">LIVE CAMERA</div><h1>画面</h1><p>IMX219 · 机器人视角</p></div>
      <div class="camera-actions"><span class="camera-status" :class="{ live }">● {{ status }}</span><button class="button" @click="connect">连接 / 重连</button><button class="button" :disabled="!running" @click="stop">停止观看</button><button class="button" @click="fullscreen">全屏</button></div>
    </header>
    <div class="camera-stage">
      <video ref="video" autoplay muted playsinline controls aria-label="机器人实时视频" />
      <div v-if="!live" class="camera-overlay"><strong>{{ status }}</strong><span>{{ error || '等待实时视频帧' }}</span></div>
    </div>
    <div class="camera-metrics"><span>分辨率 <b>{{ resolution }}</b></span><span>接收帧率 <b>{{ fps }} fps</b></span><span>接收码率 <b>{{ bitrate }} Mbps</b></span><span>WebRTC · 局域网实时视频</span></div>
    <p class="camera-note">当前使用已实测的固定曝光与白平衡，光线变化时画面亮度和色彩可能变化。离开本页自动停止观看。</p>
  </section>
</template>

<style scoped>
.camera-page{height:calc(100dvh - 125px);min-height:400px;display:flex;flex-direction:column;gap:16px}
.camera-heading{height:auto;padding:0;background:transparent;border:0;flex-shrink:0;display:flex;align-items:center;justify-content:space-between;gap:12px}.camera-heading h1{margin:5px 0;font-size:30px}.camera-heading p,.camera-note{color:var(--muted,#6b7972);margin:0;font-size:12px}.camera-actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}.camera-status{background:#fff0db;color:#96611c;border-radius:20px;padding:8px 12px;font-size:12px}.camera-status.live{background:#e1f3e7;color:#267448}.camera-stage{position:relative;flex:1;min-height:0;background:#101b17;border-radius:16px;overflow:hidden;display:flex;align-items:center;justify-content:center}.camera-stage video{width:100%;height:100%;object-fit:contain}.camera-overlay{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:12px;background:#101b17d9;color:#e3eee8;pointer-events:none}.camera-overlay span{font-size:13px}.camera-metrics{display:flex;gap:24px;flex-wrap:wrap;color:#6b7972;font-size:12px}.camera-metrics b{color:#284b3a;font-variant-numeric:tabular-nums;margin-left:8px}@media(max-width:800px){.camera-heading{align-items:flex-start;flex-direction:column}.camera-metrics{gap:10px}.camera-page{height:calc(100dvh - 110px)}}
</style>
