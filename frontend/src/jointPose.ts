import { computed, ref, watch } from "vue";
import { defineStore } from "pinia";
import { useTelemetry } from "./store";

export const SERVO_IDS = [10,11,12,13,14,20,21,22,23,24,30,31,32,33,34];
export function relativeJointAngle(position: number, reference: number, direction: number) {
  // Preserve multi-turn positions. Do not wrap encoder counts at 4096.
  if (![position, reference].every(Number.isFinite) || ![-1, 1].includes(direction))
    throw new Error("Invalid joint calibration");
  return (position - reference) * Math.PI * 2 / 4096 * direction;
}
export const useJointPose = defineStore("joint-pose", () => {
  const telemetry = useTelemetry();
  const references = ref<Record<number, number>>({});
  const directions = ref<Record<number, number>>({});
  const selected = ref(24);
  let context = "";
  watch(() => telemetry.joints, sample => {
    if (!sample) return;
    const next = JSON.stringify([telemetry.endpoint, sample.bootId, sample.source]);
    if (context && context !== next) {
      references.value = {};
      directions.value = {};
    }
    context = next;
  }, { flush: "sync" });
  function row(id: number) {
    return telemetry.joints?.data.servos.find((r: any) => r.id === id);
  }
  function fresh(id: number) {
    const r = row(id);
    return telemetry.connection === "在线" && !!telemetry.joints?.valid &&
      !!r?.online && Number.isFinite(r.position) && r.ageMs + telemetry.jointsAge < 1500;
  }
  function canCalibrate(id: number) {
    return SERVO_IDS.includes(id) && !telemetry.paused && fresh(id) && row(id).fault === 0;
  }
  function calibrate(id: number) {
    if (!canCalibrate(id)) return;
    references.value = { ...references.value, [id]: row(id).position };
  }
  function clear(id: number) {
    if (telemetry.paused) return;
    const next = { ...references.value };
    delete next[id];
    references.value = next;
  }
  function direction(id: number) { return directions.value[id] ?? -1; }
  function reverse(id: number) {
    if (telemetry.paused) return;
    directions.value = { ...directions.value, [id]: -direction(id) };
  }
  const angles = computed<Record<number, number | null>>(() => Object.fromEntries(
    SERVO_IDS.map(id => [id, references.value[id] === undefined ? 0 :
      fresh(id) ? relativeJointAngle(row(id).position, references.value[id], direction(id)) : null])
  ));
  const calibratedCount = computed(() => Object.keys(references.value).length);
  return { references, selected, angles, calibratedCount, fresh, canCalibrate, calibrate, clear, direction, reverse };
});
