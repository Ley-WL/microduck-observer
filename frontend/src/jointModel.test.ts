import { expect, it } from "vitest";
import * as THREE from "three";
import { createJointNode, applyJointAngle, smoothJointAngle } from "./jointModel";
it("smooths multi-turn angles consistently at 30 and 60 fps and holds stale data", () => {
  const run = (fps: number) => {
    let angle = 0;
    for (let i = 0; i < fps; i++) angle = smoothJointAngle(angle, 4 * Math.PI, 1 / fps);
    return angle;
  };
  expect(run(30)).toBeCloseTo(run(60), 10);
  expect(run(60)).toBeCloseTo(4 * Math.PI, 3);
  const first = smoothJointAngle(0, 1, 1 / 60);
  expect(first).toBeGreaterThan(0); expect(first).toBeLessThan(1);
  expect(smoothJointAngle(first, null, 1 / 60)).toBe(first);
  expect(smoothJointAngle(first, NaN, 1 / 60)).toBe(first);
  expect(smoothJointAngle(first, 2, 0)).toBe(first);
});
it("rotates descendants around local joint axes without losing body orientation", () => {
  const q = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1,0,0), Math.PI / 2);
  const joint = createJointNode({ pos: [1,0,0], quat: [q.w,q.x,q.y,q.z], joint: { id: 12, axis: [0,0,1] } });
  const child = new THREE.Group(); child.position.set(1,0,0); joint.inner.add(child);
  applyJointAngle(joint.inner, joint.axis!, Math.PI / 2);
  const p = child.getWorldPosition(new THREE.Vector3());
  expect(p.x).toBeCloseTo(1); expect(p.y).toBeCloseTo(0); expect(p.z).toBeCloseTo(1);
  expect(joint.node.quaternion.equals(q)).toBe(true);
  applyJointAngle(joint.inner, joint.axis!, null);
  expect(child.getWorldPosition(new THREE.Vector3()).distanceTo(p)).toBeCloseTo(0);
  applyJointAngle(joint.inner, joint.axis!, 0);
  expect(child.getWorldPosition(new THREE.Vector3()).x).toBeCloseTo(2);
});
