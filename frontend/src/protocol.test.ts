import { describe, expect, it } from "vitest";
import { euler, freshness, isSample, validQuaternion } from "./protocol";
describe("hardware telemetry boundaries", () => {
  it("rejects zero, non-finite, and non-unit quaternions", () => {
    for (const q of [
      [0, 0, 0, 0],
      [NaN, 0, 0, 1],
      [0, 0, 0, 2],
      [0, 0, 1],
    ])
      expect(validQuaternion(q)).toBe(false);
    expect(validQuaternion([0, 0, 0, 1])).toBe(true);
  });
  it("interprets xyzw as body-to-world right-handed rotation", () => {
    expect(euler([0, 0, Math.SQRT1_2, Math.SQRT1_2])[2]).toBeCloseTo(90);
    expect(euler([Math.SQRT1_2, 0, 0, Math.SQRT1_2])[0]).toBeCloseTo(90);
  });
  it("marks retained samples delayed/stale instead of live", () => {
    expect(freshness(501, true)).toBe("延迟");
    expect(freshness(1501, true)).toBe("过期");
    expect(freshness(0, false)).toBe("无效");
  });
  it("rejects incompatible protocol and invalid timestamps", () => {
    const s = {
      type: "sample",
      protocolVersion: 1,
      bootId: "a",
      topic: "pose",
      seq: 1,
      sampleMonoMs: 0,
      ageMs: 0,
      source: "hardware",
      valid: true,
      data: {},
    };
    expect(isSample(s)).toBeTruthy();
    expect(isSample({ ...s, protocolVersion: 2 })).toBeFalsy();
    expect(isSample({ ...s, ageMs: -1 })).toBeFalsy();
    expect(isSample({ ...s, source: "unknown" })).toBeFalsy();
  });
});
