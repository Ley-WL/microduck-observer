import { it, expect } from 'vitest';
import { Quaternion, Euler, Vector3 } from 'three';
import { sensorToTrunk, trunkToSensor } from './imuInstallation';
import { bodyRelativeQuaternion } from './calibration';
it('converts a dragged mounting orientation into the existing body rotation convention', () => {
  const c=new Quaternion().setFromEuler(new Euler(.6,-.9,1.1));
  const body=new Quaternion().setFromAxisAngle(new Vector3(1,0,0),.4);
  const sensor=c.clone().invert().multiply(body).multiply(c);
  const result=new Quaternion().fromArray(bodyRelativeQuaternion([0,0,0,1],sensor.toArray(),trunkToSensor(c.toArray())));
  expect(result.angleTo(body)).toBeLessThan(1e-7);
});
it('initializes the marker without changing a previously calibrated mounting', () => {
  const m=new Quaternion().setFromEuler(new Euler(1.2,.2,-.5));
  const restored=new Quaternion().fromArray(trunkToSensor(sensorToTrunk({mountingQuaternion:m.toArray()})));
  expect(restored.angleTo(m)).toBeLessThan(1e-7);
  expect(sensorToTrunk(null,90)[2]).toBeCloseTo(Math.SQRT1_2);
});
