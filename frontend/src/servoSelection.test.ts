import { describe, expect, it } from 'vitest';
import { MeshStandardMaterial } from 'three';
import model from '../public/model/model.json';
import { servoHousing, servoHighlight, stepAngle } from './servoSelection';

describe('servo fine adjustment and selection', () => {
  it('accumulates rapid clicks and respects asymmetric and mouth limits', () => {
    let angle = 0.1;
    for (let i = 0; i < 5; i++) angle = stepAngle(angle, 1, [-25, 30])!;
    expect(angle).toBe(5.1);
    expect(stepAngle(29.6, 1, [0, 30])).toBe(30);
    expect(stepAngle(0, -1, [0, 30])).toBe(0);
    expect(stepAngle(-24.8, -1, [-25, 30])).toBe(-25);
    expect(stepAngle('', 1, [0, 30])).toBeNull();
    expect(stepAngle(NaN, 1, [0, 30])).toBeNull();
  });
  it('maps all 15 joints to distinct physical servo housings in the shipped model', () => {
    expect(Object.keys(servoHousing)).toHaveLength(15);
    expect(new Set(Object.values(servoHousing).map(([name, index]) => `${name}:${index}`)).size).toBe(15);
    for (const [id, [name, index]] of Object.entries(servoHousing)) {
      expect(model.bodies.some(b => b.joint?.id === Number(id))).toBe(true);
      expect(model.bodies.find(b => b.name === name)?.geoms[index]?.mesh).toBe('xl330');
    }
  });
  it('restores original material when switching selection or clearing it', () => {
    const a = new MeshStandardMaterial({ color: '#234567', emissive: '#112233', emissiveIntensity: .2 });
    const b = new MeshStandardMaterial({ color: '#abcdef' });
    const select = servoHighlight();
    const original = a.color.clone(), glow = a.emissive.clone();
    select(a); expect(a.color.equals(original)).toBe(false);
    select(b); expect(a.color.equals(original)).toBe(true);
    expect(a.emissive.equals(glow)).toBe(true); expect(a.emissiveIntensity).toBe(.2);
    select(); expect(b.color.getHexString()).toBe('abcdef');
    a.dispose(); b.dispose();
  });
});
