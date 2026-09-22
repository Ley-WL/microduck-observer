export type Sample = {
  type: "sample";
  protocolVersion: number;
  bootId: string;
  topic: string;
  seq: number;
  sampleMonoMs: number;
  ageMs: number;
  source: "simulation" | "hardware" | "replay";
  valid: boolean;
  data: Record<string, any>;
};
export const vector = (v: unknown, n: number): v is number[] =>
  Array.isArray(v) &&
  v.length === n &&
  v.every((x) => typeof x === "number" && Number.isFinite(x));
export function isSample(v: any): v is Sample {
  return (
    v?.type === "sample" &&
    v.protocolVersion === 1 &&
    typeof v.bootId === "string" &&
    typeof v.topic === "string" &&
    Number.isSafeInteger(v.seq) &&
    v.seq >= 0 &&
    Number.isFinite(v.sampleMonoMs) &&
    Number.isFinite(v.ageMs) &&
    v.ageMs >= 0 &&
    ["simulation", "hardware", "replay"].includes(v.source) &&
    typeof v.valid === "boolean" &&
    v.data &&
    typeof v.data === "object"
  );
}
export function validQuaternion(v: unknown): v is number[] {
  return vector(v, 4) && Math.abs(Math.hypot(...v) - 1) < 0.05;
}
export function euler(q: number[]): number[] {
  const n = Math.hypot(...q),
    [x, y, z, w] = q.map((v) => v / n);
  return [
    Math.atan2(2 * (w * x + y * z), 1 - 2 * (x * x + y * y)),
    Math.asin(Math.max(-1, Math.min(1, 2 * (w * y - z * x)))),
    Math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z)),
  ].map((x) => (x * 180) / Math.PI);
}
export function freshness(age: number, valid: boolean): string {
  return !valid ? "无效" : age > 1500 ? "过期" : age > 500 ? "延迟" : "实时";
}
