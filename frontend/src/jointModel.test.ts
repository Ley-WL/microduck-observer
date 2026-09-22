import { expect, it } from "vitest";
import * as THREE from "three";
import { createJointNode, applyJointAngle } from "./jointModel";
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
