import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { reactive } from "vue";
import { relativeJointAngle, useJointPose } from "./jointPose";
const { state } = vi.hoisted(() => ({ state: { current: null as any, board:null as any } }));
vi.mock("./boardCalibration", () => ({useBoardCalibration:()=>state.board}));
vi.mock("./store", () => ({ useTelemetry: () => state.current }));
beforeEach(() => {
  const saved = new Map<string, string>();
  vi.stubGlobal("localStorage", { getItem: (k: string) => saved.get(k) ?? null, setItem: (k: string, v: string) => saved.set(k,v) });
  setActivePinia(createPinia());
  state.board=reactive({data:{joints:{references:{},directions:{}}},ready:true,saving:false,error:"",joints(edit:any){edit(this.data.joints);}});
  state.current = reactive({ endpoint: "robot", connection: "在线", paused: false, jointsAge: 0, joints: null });
});
afterEach(() => vi.restoreAllMocks());
function sample(position = 4971, bootId = "a") {
  return { bootId, source: "hardware", valid: true, data: { servos: [
    { id: 12, online: true, position, ageMs: 0, fault: 0 },
    { id: 24, online: true, position: 2000, ageMs: 0, fault: 0 },
  ] } };
}
it("keeps multi-turn encoder deltas and validates inputs", () => {
  expect(relativeJointAngle(4971, 4971, -1)).toBeCloseTo(0);
  expect(relativeJointAngle(9067, 4971, -1)).toBeCloseTo(-2 * Math.PI);
  expect(relativeJointAngle(5995, 4971, 1)).toBeCloseTo(Math.PI / 2);
  expect(() => relativeJointAngle(NaN, 0, 1)).toThrow();
});
it("calibrates independently, follows feedback, reverses and clears", () => {
  const pose = useJointPose();
  state.current.joints = sample();
  pose.calibrate(12);
  expect(pose.references[12]).toBe(4971);
  expect(pose.references[24]).toBeUndefined();
  state.current.joints = sample(5995);
  expect(pose.angles[12]).toBeCloseTo(-Math.PI / 2);
  pose.reverse(12);
  expect(pose.angles[12]).toBeCloseTo(Math.PI / 2);
  pose.clear(12);
  expect(pose.angles[12]).toBe(0);
});
it("blocks unsafe references, holds stale poses and retains servo references across a new boot", () => {
  const pose = useJointPose();
  state.current.joints = sample();
  pose.calibrate(12);
  state.current.jointsAge = 1500;
  expect(pose.angles[12]).toBeNull();
  expect(pose.canCalibrate(24)).toBe(false);
  state.current.jointsAge = 0;
  state.current.paused = true;
  pose.calibrate(24);
  expect(pose.references[24]).toBeUndefined();
  state.current.paused = false;
  state.current.joints.data.servos[1].fault = 1;
  expect(pose.canCalibrate(24)).toBe(false);
  state.current.connection = "离线";
  expect(pose.angles[12]).toBeNull();
  state.current.joints = sample(4971, "b");
  expect(pose.calibratedCount).toBe(1);
});
