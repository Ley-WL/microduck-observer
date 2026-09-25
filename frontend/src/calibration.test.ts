import { describe, expect, it } from "vitest";
import { relativeQuaternion, bodyRelativeQuaternion } from "./calibration";
import { euler } from "./protocol";

describe("initial orientation reference", () => {
  it("shows a supine body at the captured reference instead of standing upright", () => {
    const q=[.5,.5,.5,.5], target=[0,-Math.SQRT1_2,0,Math.SQRT1_2];
    const result=bodyRelativeQuaternion(q,q,undefined,target);
    result.forEach((v,i)=>expect(v).toBeCloseTo(target[i]));
  });
  it("maps sensor-axis rotation into the calibrated body frame", () => {
    const h=Math.SQRT1_2;
    const result=euler(bodyRelativeQuaternion([0,0,0,1],[0,h,0,h],[0,0,h,h]));
    expect(result[0]).toBeCloseTo(90);expect(result[1]).toBeCloseTo(0);expect(result[2]).toBeCloseTo(0);
  });
  it("zeroes any initial mounting attitude", () => {
    const q = [0.2, 0.3, 0.4, Math.sqrt(0.71)];
    const r = relativeQuaternion(q, q);
    r.forEach((v, i) => expect(v).toBeCloseTo(i === 3 ? 1 : 0));
  });
  it("uses initial sensor axes, including a non-identity initial orientation", () => {
    const s = Math.SQRT1_2;
    // q0 = 90 degrees about Z; current = q0 * 90 degrees about X.
    const result = euler(
      relativeQuaternion([0, 0, s, s], [0.5, 0.5, 0.5, 0.5]),
    );
    expect(result[0]).toBeCloseTo(90);
    expect(result[1]).toBeCloseTo(0);
    expect(result[2]).toBeCloseTo(0);
  });
  it("handles the equivalent negative quaternion without a 360-degree jump", () => {
    const result = euler(relativeQuaternion([0, 0, 0, 1], [0, 0, 0, -1]));
    result.forEach((v) => expect(v).toBeCloseTo(0));
  });
  it("rejects a missing or invalid sensor orientation", () => {
    expect(() => relativeQuaternion([0, 0, 0, 1], [0, 0, 0, 0])).toThrow();
  });
});
