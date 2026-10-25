import { describe, expect, it } from 'vitest';
import { rollerBindings } from './rollerMode';

describe('roller mode bindings', () => {
  it('uses X for one-shot crouch and A for balance without changing ordinary bindings', () => {
    const saved = { a: 'standup', x: 'roulade', rb: 'quack' };
    expect(rollerBindings(saved, true)).toEqual({ a: 'swizzle', x: 'roller_crouch', rb: 'quack' });
    expect(rollerBindings(saved, false)).toBe(saved);
    expect(saved.x).toBe('roulade');
  });
  it('exposes only the fixed overrides before saved bindings load', () => {
    expect(rollerBindings(undefined, true)).toEqual({ a: 'swizzle', x: 'roller_crouch' });
    expect(rollerBindings(undefined, false)).toBeUndefined();
  });
});
