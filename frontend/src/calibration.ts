import { validQuaternion } from "./protocol";
import { Quaternion } from "three";

export function bodyRelativeQuaternion(reference: number[], current: number[], mounting?: number[], target?: number[]) {
  const result = new Quaternion().fromArray(relativeQuaternion(reference, current));
  if (validQuaternion(mounting)) {
    const m = new Quaternion().fromArray(mounting).normalize();
    result.premultiply(m.clone().invert()).multiply(m);
  }
  if (validQuaternion(target)) result.premultiply(new Quaternion().fromArray(target).normalize());
  return result.normalize().toArray();
}

/** Display rotation in the initial sensor frame: inverse(reference) * current.
 * This zeroes orientation only; it does not determine the sensor mounting axes.
 */
export function relativeQuaternion(
  reference: number[],
  current: number[],
): number[] {
  if (!validQuaternion(reference) || !validQuaternion(current)) {
    throw new Error("标定需要有效的单位四元数");
  }
  const a = reference.map((v) => v / Math.hypot(...reference));
  const b = current.map((v) => v / Math.hypot(...current));
  const [x, y, z, w] = [-a[0], -a[1], -a[2], a[3]];
  const [u, v, s, t] = b;
  const q = [
    w * u + x * t + y * s - z * v,
    w * v - x * s + y * t + z * u,
    w * s + x * v - y * u + z * t,
    w * t - x * u - y * v - z * s,
  ];
  const n = Math.hypot(...q);
  return q.map((value) => value / n);
}
