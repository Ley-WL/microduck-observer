import type { MeshStandardMaterial } from 'three';

// Servo housings belong to the fixed side of each joint in the exported model.
export const servoHousing: Record<number, readonly [string, number]> = {
  20: ['trunk_base', 2], 21: ['yaw2roll', 1], 22: ['upper_leg_left', 2],
  23: ['upper_leg_left', 1], 24: ['leg', 1],
  10: ['trunk_base', 4], 11: ['bearing_roll', 1], 12: ['upper_leg_right', 2],
  13: ['upper_leg_right', 0], 14: ['leg_2', 0],
  30: ['neck', 1], 31: ['neck', 2], 32: ['yaw_roll_motion', 1],
  33: ['jaw_soft', 4], 34: ['jaw_soft', 2],
};

export function stepAngle(raw: number | string, delta: number, range: readonly [number, number]) {
  if (raw === '' || !Number.isFinite(Number(raw))) return null;
  return Math.min(range[1], Math.max(range[0], Math.round((Number(raw) + delta) * 10) / 10));
}

export function servoHighlight() {
  let restore: (() => void) | undefined;
  return (material?: MeshStandardMaterial) => {
    restore?.(); restore = undefined;
    if (!material) return;
    const color = material.color.clone(), emissive = material.emissive.clone();
    const intensity = material.emissiveIntensity;
    restore = () => { material.color.copy(color); material.emissive.copy(emissive); material.emissiveIntensity = intensity; };
    material.color.set('#ffad32');
    material.emissive.set('#ff8800');
    material.emissiveIntensity = .65;
  };
}
