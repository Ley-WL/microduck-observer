import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { useTelemetry } from "./store";
import { useBoardCalibration } from "./boardCalibration";

export const SERVO_IDS = [10,11,12,13,14,20,21,22,23,24,30,31,32,33,34];
export function relativeJointAngle(position: number, reference: number, direction: number) {
  // Preserve multi-turn positions. Do not wrap encoder counts at 4096.
  if (![position, reference].every(Number.isFinite) || ![-1, 1].includes(direction))
    throw new Error("Invalid joint calibration");
  return (position - reference) * Math.PI * 2 / 4096 * direction;
}
export const useJointPose = defineStore("joint-pose", () => {
  const telemetry = useTelemetry();
  const board = useBoardCalibration();
  const references = computed<Record<number, number>>(() => board.data?.joints.references || {});
  const directions = computed<Record<number, number>>(() => board.data?.joints.directions || {});
  const selected = ref(24);
  const saveError = computed(() => !!board.error);
  function row(id: number) {
    return telemetry.joints?.data.servos.find((r: any) => r.id === id);
  }
  function fresh(id: number) {
    const r = row(id);
    return telemetry.connection === "在线" && !!telemetry.joints?.valid &&
      !!r?.online && Number.isFinite(r.position) && r.ageMs + telemetry.jointsAge < 1500;
  }
  function canCalibrate(id: number) {
    return board.ready && !board.saving && SERVO_IDS.includes(id) && !telemetry.paused && fresh(id) && row(id).fault === 0;
  }
  function calibrate(id: number) {
    if (!canCalibrate(id)) return;
    const position=row(id).position;
    return board.joints(j=>{j.references[id]=position;});
  }
  function clear(id: number) {
    if (telemetry.paused || !board.ready) return;
    return board.joints(j=>{delete j.references[id];});
  }
  function direction(id: number) { return directions.value[id] ?? -1; }
  function reverse(id: number) {
    if (telemetry.paused || !board.ready) return;
    return board.joints(j=>{j.directions[id]=-(j.directions[id]??-1);});
  }
  const angles = computed<Record<number, number | null>>(() => Object.fromEntries(
    SERVO_IDS.map(id => [id, references.value[id] === undefined ? 0 :
      fresh(id) ? relativeJointAngle(row(id).position, references.value[id], direction(id)) : null])
  ));
  const calibratedCount = computed(() => Object.keys(references.value).length);
  return { references, selected, angles, calibratedCount, saveError, fresh, canCalibrate, calibrate, clear, direction, reverse };
});
