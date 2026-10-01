import { Quaternion, Vector3 } from 'three';
import { validQuaternion } from './protocol';

// Marker orientation C maps sensor-local axes into the trunk frame.
// Persisted mounting M maps trunk axes into sensor axes: M = inverse(C).
export function sensorToTrunk(imu: any, yaw = 0): number[] {
  if (validQuaternion(imu?.mountingQuaternion))
    return new Quaternion().fromArray(imu.mountingQuaternion).normalize().invert().toArray();
  return new Quaternion().setFromAxisAngle(new Vector3(0,0,1), yaw*Math.PI/180).toArray();
}
export function trunkToSensor(marker: number[]): number[] {
  if (!validQuaternion(marker)) throw new Error('安装方向无效');
  return new Quaternion().fromArray(marker).normalize().invert().toArray();
}
