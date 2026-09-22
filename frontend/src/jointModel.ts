import * as THREE from "three";

export function createJointNode(body: { pos: number[]; quat: number[]; joint?: { axis: number[]; id: number } }) {
  const node = new THREE.Group();
  node.position.fromArray(body.pos);
  node.quaternion.set(body.quat[1], body.quat[2], body.quat[3], body.quat[0]).normalize();
  const inner = body.joint ? new THREE.Group() : node;
  const axis = body.joint ? new THREE.Vector3().fromArray(body.joint.axis).normalize() : null;
  if (inner !== node) node.add(inner);
  return { node, inner, axis };
}
export function applyJointAngle(pivot: THREE.Group, axis: THREE.Vector3, angle: number | null | undefined) {
  // Missing/stale samples retain the last displayed angle. Explicit zero clears it.
  if (typeof angle === "number" && Number.isFinite(angle)) pivot.quaternion.setFromAxisAngle(axis, angle);
}
