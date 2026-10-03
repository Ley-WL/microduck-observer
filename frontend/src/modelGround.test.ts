import { expect, it } from "vitest";
import * as THREE from "three";
import { placeOnGround } from "./modelGround";

it("places articulated tilted feet on ground without flattening their relative height", () => {
  const root = new THREE.Group();
  root.position.z = .12;
  root.quaternion.setFromAxisAngle(new THREE.Vector3(0,1,0), .2);
  const feet = [-1,1].map(side => {
    const pivot = new THREE.Group();
    pivot.position.set(side * .05, 0, -.08);
    pivot.rotation.y = side * .3;
    const foot = new THREE.Mesh(new THREE.BoxGeometry(.07,.04,.02));
    pivot.add(foot); root.add(pivot); return foot;
  });
  const min = (foot: THREE.Mesh) => {
    const p = foot.geometry.getAttribute('position');
    return Math.min(...Array.from({length:p.count}, (_,i) => new THREE.Vector3().fromBufferAttribute(p,i).applyMatrix4(foot.matrixWorld).z));
  };
  root.updateMatrixWorld(true);
  const difference = min(feet[0]!) - min(feet[1]!);
  placeOnGround(root, feet);
  expect(Math.min(...feet.map(min))).toBeCloseTo(0, 12);
  expect(min(feet[0]!) - min(feet[1]!)).toBeCloseTo(difference, 12);
  const height = root.position.z;
  placeOnGround(root, feet);
  expect(root.position.z).toBeCloseTo(height, 12);
});
