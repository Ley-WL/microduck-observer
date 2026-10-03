import * as THREE from "three";

/** Display placement only: no absolute body height/contact sensor is available. */
export function placeOnGround(root: THREE.Object3D, soles: THREE.Mesh[], groundZ = 0): void {
  root.updateMatrixWorld(true);
  let lowest = Infinity;
  for (const mesh of soles) {
    const positions = mesh.geometry.getAttribute("position");
    const e = mesh.matrixWorld.elements;
    for (let i = 0; i < positions.count; i++) {
      lowest = Math.min(lowest, e[2]! * positions.getX(i) + e[6]! * positions.getY(i) + e[10]! * positions.getZ(i) + e[14]!);
    }
  }
  if (Number.isFinite(lowest)) {
    root.position.z += groundZ - lowest;
    root.updateMatrixWorld(true);
  }
}
