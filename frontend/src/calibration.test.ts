import { describe, expect, it } from "vitest";
import { relativeQuaternion, bodyRelativeQuaternion, mountingQuaternion } from "./calibration";
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


describe("trunk IMU mounting axes", () => {
  const h = Math.PI / 36; // 10 degrees rotation, half-angle 5 degrees.
  it("maps observed left roll to the commanded physical forward pitch", () => {
    const angles = euler(mountingQuaternion([-Math.sin(h), 0, 0, Math.cos(h)], -90));
    expect(angles[0]).toBeCloseTo(0);
    expect(angles[1]).toBeCloseTo(10);
  });
  it("maps observed backward pitch to physical left roll", () => {
    const angles = euler(mountingQuaternion([0, -Math.sin(h), 0, Math.cos(h)], -90));
    expect(angles[0]).toBeCloseTo(-10);
    expect(angles[1]).toBeCloseTo(0);
  });
  it("preserves yaw, calibrated zero, unit length and permits rollback", () => {
    const q = [0, 0, Math.sin(h), Math.cos(h)];
    expect(euler(mountingQuaternion(q, -90))[2]).toBeCloseTo(10);
    expect(mountingQuaternion([0, 0, 0, 1], -90)).toEqual([0, 0, 0, 1]);
    const mixed = [.2, .3, .4, Math.sqrt(.71)];
    const rotated = mountingQuaternion(mixed, -90);
    expect(Math.hypot(...rotated)).toBeCloseTo(1);
    mountingQuaternion(rotated, 90).forEach((v, i) => expect(v).toBeCloseTo(mixed[i]));
    mountingQuaternion(mixed, 0).forEach((v, i) => expect(v).toBeCloseTo(mixed[i]));
  });
  it("rejects invalid inputs", () => {
    expect(() => mountingQuaternion([0, 0, 0, 0], -90)).toThrow();
    expect(() => mountingQuaternion([0, 0, 0, 1], 30)).toThrow();
  });
});


import { savedDisplayReference } from "./calibration";
it("restores a persisted reference for display without changing its session or data", () => {
  const imu = { quaternion: [0, 0, 0, 1], bootId: "previous-service" };
  expect(savedDisplayReference(imu)).toEqual([0, 0, 0, 1]);
  expect(savedDisplayReference(imu)).not.toBe(imu.quaternion);
  expect(imu.bootId).toBe("previous-service");
  expect(savedDisplayReference({ quaternion: [0, 0, 0, 0] })).toBeNull();
  expect(savedDisplayReference({ quaternion: null })).toBeNull();
});
