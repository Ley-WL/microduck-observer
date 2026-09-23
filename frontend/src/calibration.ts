import { validQuaternion } from "./protocol";

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

/** Change relative sensor rotation into the robot frame: C * q * inverse(C).
 * C rotates sensor axes around +Z. Unlike subtracting Euler angles, this also
 * preserves coupled rotations and keeps the calibrated identity unchanged.
 */
export function mountingQuaternion(q: number[], yawDegrees: number): number[] {
  if (!validQuaternion(q) || ![0, -90, 90].includes(yawDegrees)) {
    throw new Error("无效的安装方向或四元数");
  }
  const n = Math.hypot(...q);
  const [x, y, z, w] = q.map((v) => v / n);
  const angle = yawDegrees * Math.PI / 180;
  return [Math.cos(angle) * x - Math.sin(angle) * y,
          Math.sin(angle) * x + Math.cos(angle) * y, z, w];
}

/** Persisted display reference may be previewed across service sessions.
 * Consumers must separately require matching boot IDs before policy inference.
 */
export function savedDisplayReference(imu: any): number[] | null {
  return validQuaternion(imu?.quaternion) ? [...imu.quaternion] : null;
}
