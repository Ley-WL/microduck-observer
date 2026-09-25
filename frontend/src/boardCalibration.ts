import { ref, watch, onScopeDispose } from "vue";
import { defineStore } from "pinia";
import { useTelemetry } from "./store";
import { calibrationKey, loadCalibration } from "./savedCalibration";

export const useBoardCalibration = defineStore("board-calibration", () => {
  const telemetry = useTelemetry();
  const data = ref<any>(null), ready = ref(false), saving = ref(false), error = ref("");
  let generation = 0, loading = false;
  let writes: Promise<void> = Promise.resolve();
  const base = () => telemetry.endpoint.replace(/\/$/, "");
  async function request(url: string, options?: RequestInit) {
    const response = await fetch(url, { ...options, signal: AbortSignal.timeout(5000) });
    const value = await response.json();
    if (!response.ok) throw new Error(value.detail || "主板标定保存失败");
    if (value.schema !== 1 || !Number.isInteger(value.revision) || !value.joints || !value.imu) throw new Error("主板标定格式不兼容");
    return value;
  }
  async function refresh() {
    if (loading || saving.value) return;
    loading = true; const id = generation, url = base();
    try { const result = await request(url + "/api/v1/calibration"); if (id === generation) { if (!data.value || result.deviceId !== data.value.deviceId || result.revision >= data.value.revision) data.value = result; ready.value = true; error.value = ""; } }
    catch (e) { if (id === generation) error.value = String(e); }
    finally { if (id === generation) loading = false; }
  }
  function write(make: (current: any) => any, migrate = false) {
    const id = generation, url = base();
    const run = async () => {
      if (id !== generation || !ready.value) return;
      saving.value = true;
      try {
        const patch = make(data.value);
        if (!Object.keys(patch).length) return;
        const result = await request(url + "/api/v1/calibration", { method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ revision: data.value.revision, patch, migrate }) });
        if (id === generation) { data.value = result; error.value = ""; }
      } catch (e) { if (id === generation) { error.value = String(e); const problem=error.value;
          try { const latest=await request(url+"/api/v1/calibration"); if(id===generation)data.value=latest; } catch { /* Retain last confirmed board state. */ }
          if(id===generation)error.value=problem;
        } }
      finally { if (id === generation) saving.value = false; }
    };
    writes = writes.then(run, run); return writes;
  }
  function joints(edit: (j: any) => void) {
    return write(current => { const j = JSON.parse(JSON.stringify(current.joints)); edit(j); return { joints: { references: j.references, directions: j.directions } }; });
  }
  function orientation(quaternion: number[] | null, bootId: string, time: string) {
    return write(() => ({ imu: { quaternion, bootId, time } }));
  }
  function confirmReference(bootId: string) {
    return write(current => {
      const { validForBoot: _validForBoot, ...imu } = current.imu;
      return { imu: { ...imu, bootId } };
    });
  }
  function mounting(yaw: number) {
    if (![0, -90, 90].includes(yaw)) return;
    return write(() => ({ mounting: { yaw } }));
  }
  let migrationKey = "";
  function migrateLegacy() {
    if (!ready.value || !data.value || saving.value) return;
    const patch: any = {};
    if (!data.value.joints.initialized && telemetry.joints?.source === "hardware") {
      const saved = loadCalibration(calibrationKey("joints", [base(), "hardware"]));
      if (saved && (Object.keys(saved.references || {}).length || Object.keys(saved.directions || {}).length)) patch.joints = { references: saved.references || {}, directions: saved.directions || {} };
    }
    const sample = telemetry.orientation;
    if (!data.value.imu.initialized && sample?.source === "hardware" && sample.bootId === data.value.bootId) {
      const saved = loadCalibration(calibrationKey("imu", [base(), sample.bootId, sample.data.frame, sample.source]));
      if (Array.isArray(saved?.quaternion)) patch.imu = { quaternion: saved.quaternion, bootId: sample.bootId, time: saved.time || "" };
    }
    const key=base()+JSON.stringify(patch);
    if (!Object.keys(patch).length || key===migrationKey) return;
    migrationKey=key;
    void write(current=>Object.fromEntries(Object.entries(patch).filter(([section])=>!current[section].initialized)),true);
  }
  watch(() => telemetry.endpoint, () => { generation++; loading=false; data.value=null; ready.value=false; saving.value=false; error.value=""; migrationKey=""; void refresh(); }, { immediate:true });
  watch([data, () => telemetry.joints?.source, () => telemetry.orientation?.bootId], migrateLegacy);
  const timer=setInterval(()=>{void refresh();},1500);onScopeDispose(()=>clearInterval(timer));
  return { data, ready, saving, error, refresh, joints, orientation, mounting, confirmReference };
});
