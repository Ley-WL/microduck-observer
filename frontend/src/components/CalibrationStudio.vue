<script setup lang="ts">
import { computed, ref, onMounted, watch } from "vue";
import RobotView from "./RobotView.vue";
import { useTelemetry } from "../store";
import { useBoardCalibration } from "../boardCalibration";
const telemetry=useTelemetry(), board=useBoardCalibration();
const poses=ref<Record<string,any>>({}), pose=ref('fold'), selected=ref<number[]>([]);
const position=ref(true), hardware=ref(false), imu=ref(false), confirmed=ref(false);
const busy=ref(false), stage=ref('准备'), error=ref(''), plan=ref<any>(null), result=ref<any>(null);
const names:Record<number,string>={10:'右髋偏航',11:'右髋横滚',12:'右髋俯仰',13:'右膝',14:'右踝',20:'左髋偏航',21:'左髋横滚',22:'左髋俯仰',23:'左膝',24:'左踝',30:'颈俯仰',31:'头俯仰',32:'头偏航',33:'头横滚',34:'嘴'};
const preset=computed(()=>poses.value[pose.value]);
const modes=computed(()=>[...(position.value?['position']:[]),...(hardware.value?['hardware']:[]),...(imu.value?['imu']:[])]);
const rows=computed(()=>telemetry.joints?.data.servos || []);
const online=(id:number)=>telemetry.connection==='在线' && telemetry.joints?.valid && rows.value.some((r:any)=>r.id===id && r.online && !r.fault && r.ageMs+telemetry.jointsAge<350);
const baseline=(id:number)=>board.data?.joints.references[id]!==undefined;
const count=computed(()=>Object.keys(board.data?.joints.references || {}).length);
const canCapture=computed(()=>!busy.value && board.ready && !telemetry.paused && telemetry.connection==='在线' && modes.value.length>0 && (!(position.value || hardware.value) || selected.value.length>0));
const instructions=computed(()=>pose.value==='head'
 ? ['支撑躯干；颈部摆正，头平视正前方，嘴轻合。','头部偏航与横滚居中；不要借机械限位强行掰头。','只采集头颈与嘴，不覆盖已完成的腿部标定。']
 : pose.value.startsWith('imu')
 ? ['先把躯干放在水平支撑面上，面朝你选定的正前方。','躯干固定在支撑面上，不依靠头或脚承重；保持静止。','先水平采集，再按需做侧放和低头姿势，识别安装轴向。']
 : ['将躯干托稳，腿按中间模型折叠；不要强推关节到限位。','髋、膝为模型 ±89.95°；头颈摆正、嘴闭合。','脚踝与髋横滚、偏航按模型对齐；不易摆齐的关节取消勾选，稍后单独补齐。']);
function invalidate(){plan.value=null;result.value=null;confirmed.value=false;stage.value='准备';error.value='';}
watch([pose,position,hardware,imu,selected],invalidate,{deep:true});
watch(pose,()=>{selected.value=[...(preset.value?.ids || [])];if(pose.value.startsWith('imu')){position.value=false;hardware.value=false;imu.value=true;}});
watch(()=>telemetry.endpoint,()=>{invalidate();void load();});
async function api(path:string,body?:any){
 const response=await fetch(telemetry.endpoint.replace(/\/$/,'')+'/api/v1/calibration/'+path,body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{});
 const data=await response.json();if(!response.ok)throw new Error(data.detail || '标定请求失败');return data;
}
async function load(){try{poses.value=await api('poses');selected.value=[...(preset.value?.ids || [])];}catch(e){error.value=String(e);}}
onMounted(load);
async function capture(){
 busy.value=true;stage.value='采样中 · 请静止';error.value='';result.value=null;plan.value=null;confirmed.value=false;
 try{plan.value=await api('preview',{pose:pose.value,ids:selected.value,modes:modes.value});stage.value='预览待确认';}
 catch(e){error.value=String(e);stage.value='采样未通过';}finally{busy.value=false;}
}
async function execute(){
 if(!plan.value || !confirmed.value)return;
 busy.value=true;stage.value=hardware.value?'写入中 · 请勿断电或移动':'保存中';error.value='';
 try{result.value=await api('execute',{token:plan.value.token,confirm:true});stage.value=result.value.ok?'已完成':'部分失败 · 查看逐颗结果';await board.refresh();}
 catch(e){error.value=String(e);stage.value='执行未完成 · 重新采样前检查状态';}
 finally{busy.value=false;plan.value=null;confirmed.value=false;}
}
const targetOrientation=computed(()=>pose.value==='imu-roll'?[Math.SQRT1_2,0,0,Math.SQRT1_2]:pose.value==='imu-pitch'?[0,Math.SQRT1_2,0,Math.SQRT1_2]:[0,0,0,1]);
</script>
<template>
 <section class="calibration-studio">
  <div class="studio-heading"><div><h1>姿势标定</h1><span>摆好姿势，采样一次，批量保存</span></div><b>{{stage}}</b><span>位置参考 {{count}} / 15</span></div>
  <div class="studio-grid">
   <section class="studio-panel setup"><h2>01 选择姿势与项目</h2>
    <label>标定姿势<select v-model="pose" :disabled="busy"><option v-for="(p,key) in poses" :value="key">{{p.name}}</option></select></label>
    <div class="mode-options">
     <label><input type="checkbox" v-model="position" :disabled="busy || pose.startsWith('imu')"/>位置标定 <small>主控保存显示零点，不改编码器</small></label>
     <label><input type="checkbox" v-model="hardware" :disabled="busy || pose.startsWith('imu')"/>硬件中位校准 <small>写舵机 EEPROM；零位统一为 2048</small></label>
     <label><input type="checkbox" v-model="imu" :disabled="busy"/>IMU 姿态标定 <small>水平参考；可追加两姿势安装轴向校准</small></label>
    </div>
    <h2>摆放指南</h2><ol><li v-for="line in instructions">{{line}}</li></ol>
    <p v-if="pose==='imu-roll'" class="notice">从水平参考姿势向右侧放：绕身体前向 +X 旋转 +90°，左侧朝上。保持原来朝向，不额外转动躯干。</p>
    <p v-if="pose==='imu-pitch'" class="notice">从水平参考姿势低头：绕身体左向 +Y 旋转 +90°，头朝下。用支架支撑，保持静止。</p>
    <p v-if="hardware" class="notice">所选舵机必须扭矩关闭并有支撑。不会自动运动或开关扭矩；中位校准同步更新显示零点，防止模型沿用旧坐标。</p>
    <p class="hint">折叠姿势来自项目上游 FOLD_CALIB。它是参考姿势，不是已经验证适合你这台装配的机械夹具。</p>
   </section>
   <section class="studio-panel reference"><div class="reference-head"><h2>02 对照目标姿势</h2><span>目标预览 · 不驱动实物</span></div>
    <div class="reference-model"><RobotView :quaternion="targetOrientation" :paused="false" :preview-angles="preset?.angles || {}" /></div>
    <div class="capture-area"><p>采集约 1.3 秒；检查关节稳定、在线状态及 IMU 重力。预览有效期 60 秒。</p>
     <button class="primary" :disabled="!canCapture" @click="capture">{{busy?'处理中…':'一键采样并预览'}}</button>
     <p v-if="error" class="error" role="alert">{{error}}</p>
     <template v-if="plan"><label class="confirm"><input type="checkbox" v-model="confirmed" :disabled="busy"/>我已核对实物与模型姿势、关节方向{{hardware?'，确认写入硬件中位':''}}</label>
      <button class="primary" :disabled="busy || !confirmed" @click="execute">{{hardware?'执行硬件校准并保存':'保存所选标定'}}</button>
     </template>
     <p v-if="result">{{result.ok?'标定已保存到主控，刷新不会丢失。':'部分校准失败，已停止后续写入；不要直接重复执行。'}}<br v-if="result.backup"/><span v-if="result.backup">备份编号：{{result.backup}}</span></p>
    </div>
   </section>
   <section class="studio-panel selection"><h2>03 关节与预览</h2><p>仅更新勾选关节，其他关节保持原标定。</p>
    <div class="joint-list"><label v-for="id in preset?.ids || []" :key="id" class="joint-line"><input type="checkbox" v-model="selected" :value="id" :disabled="busy"/><b>{{id}} {{names[id]}}</b><span :class="online(id)?'online':'offline'">{{online(id)?'在线':'无新鲜反馈'}}</span><small>{{baseline(id)?'已有参考':'未标定'}}</small></label></div>
    <div v-if="plan" class="preview-list"><div v-for="r in plan.rows"><b>#{{r.id}}</b><span>{{Math.round(r.position)}} → {{hardware?r.target:Math.round(r.reference)}} 步</span><small>{{hardware?'写入目标':'模型零位参考'}} · 方向 {{r.direction}}</small></div><p v-if="plan.quaternion">IMU 稳定采样通过</p></div>
    <div v-if="result?.results?.length" class="preview-list"><div v-for="r in result.results"><b>#{{r.id}} {{r.ok?'通过':'失败'}}</b><span>{{r.error || '写后位置、偏移及锁定已校验'}}</span></div></div>
    <p v-if="pose.startsWith('imu')">水平参考：{{board.data?.imu.quaternion && board.data?.imu.validForBoot?'已记录':'待采集 / 会话已变化'}}<br/>侧放：{{board.data?.imu.mountingSamples?.roll?'已记录':'待采集'}}<br/>低头：{{board.data?.imu.mountingSamples?.pitch?'已记录':'待采集'}}<br/>安装轴向：{{board.data?.imu.mountingQuaternion?'已标定':'未完成'}}</p>
    <p class="hint">每颗关节的方向 ± 沿用观测页设置。单一静止姿势无法自动判断旋转方向；先检查转向，再执行硬件中位。</p>
   </section>
  </div>
 </section>
</template>
<style scoped>
.calibration-studio{height:100%;display:flex;flex-direction:column;gap:12px;color:#284637}.studio-heading{display:flex;align-items:center;gap:22px;min-height:46px}.studio-heading h1{font-size:21px;margin:0 0 3px}.studio-heading span{font-size:11px;color:#687e6d}.studio-heading>b{margin-left:auto;font-size:12px;background:#e3eee5;padding:8px 12px;border-radius:20px}.studio-grid{display:grid;grid-template-columns:3fr 4fr 3fr;gap:12px;min-height:0;flex:1}.studio-panel{background:#fff;border:1px solid #dae5db;border-radius:10px;padding:16px;min-width:0;overflow:auto}.studio-panel h2{font-size:13px;margin:0 0 14px}.setup>label{font-size:11px;display:block}.setup select{display:block;width:100%;margin:8px 0 16px;padding:9px;border:1px solid #b5caba;border-radius:5px;background:#fafcf9;color:inherit}.mode-options{display:grid;gap:13px;margin-bottom:24px}.mode-options label{font-size:12px}.mode-options small{display:block;margin:4px 0 0 24px;color:#7a877a;font-size:10px;line-height:1.5}.studio-panel ol{padding-left:17px;font-size:12px;line-height:1.9}.notice{background:#fff5db;color:#835d21;padding:10px;border-radius:6px;font-size:11px;line-height:1.7}.hint{font-size:10px;color:#7f8e81;line-height:1.8}.reference{display:flex;flex-direction:column;padding:0;overflow:hidden}.reference-head{padding:15px;display:flex;align-items:center;justify-content:space-between}.reference-head h2{margin:0}.reference-head span{font-size:9px;color:#798d7c}.reference-model{flex:1;min-height:180px;background:#f3f7f0}.reference-model :deep(.robot-canvas){height:100%;position:relative}.capture-area{padding:14px;border-top:1px solid #e0e8dc}.capture-area p{font-size:11px;line-height:1.6;margin:0 0 10px;overflow-wrap:anywhere}.primary{width:100%;padding:11px;background:#285b40;border:0;border-radius:6px;color:white;font:inherit;font-size:12px;cursor:pointer}.primary:disabled{opacity:.4;cursor:default}.confirm{display:block;margin:12px 0;font-size:11px;line-height:1.6}.error{color:#b23131;margin-top:10px!important}.selection>p{font-size:11px;line-height:1.8}.joint-line{display:flex;gap:5px;align-items:center;border-bottom:1px solid #edf1e9;padding:7px 0;font-size:10px}.joint-line b{font-weight:500}.joint-line span{margin-left:auto;font-size:9px;border-radius:4px;padding:2px 4px}.online{background:#e1f3e7;color:#24613b}.offline{background:#fbe7e5;color:#9c4034}.joint-line small{font-size:8px;color:#889585}.preview-list{margin-top:10px;font-size:10px}.preview-list>div{padding:5px 0;display:flex;gap:8px;flex-wrap:wrap;border-bottom:1px solid #e9eee5}.preview-list small{color:#7b8c7e}.preview-list b{min-width:28px}@media(max-height:760px){.studio-panel{padding:10px}.reference{padding:0}.studio-heading{min-height:35px}.mode-options{gap:8px;margin-bottom:14px}.studio-panel ol{font-size:11px}.joint-line{padding:4px 0}.studio-panel h2{margin-bottom:9px}.capture-area{padding:10px}}
</style>
