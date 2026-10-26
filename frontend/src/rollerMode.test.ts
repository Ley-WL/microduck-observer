import { describe, expect, it } from 'vitest';
import { rollerBindings, rollerResumeReady, rollerCrouchReturnReady } from './rollerMode';

describe('roller mode bindings', () => {
  it('allows A during crouch but blocks repeated return and feedback pauses', () => {
    const control = { state: 'policy', activeSkill: 'swizzle_crouch' };
    expect(rollerCrouchReturnReady(control)).toBe(true);
    expect(rollerCrouchReturnReady({ ...control, rollerCrouchReturning: true })).toBe(false);
    expect(rollerCrouchReturnReady({ ...control, feedbackHolding: true })).toBe(false);
    expect(rollerCrouchReturnReady({ ...control, state: 'disabled' })).toBe(false);
    expect(rollerCrouchReturnReady(undefined)).toBe(false);
  });
  it('enables A only for a stable fallen roller session, including interrupted crouch', () => {
    const control = { state: 'policy', policyPhase: 'roller-fallen-wait', rollerResumeReady: true, activeSkill: 'swizzle', feedbackHolding: true };
    expect(rollerResumeReady(control)).toBe(true);
    expect(rollerResumeReady({ ...control, activeSkill: 'swizzle_crouch' })).toBe(true);
    for (const patch of [{ state: 'disabled' }, { policyPhase: 'feedback-hold' }, { rollerResumeReady: false }, { activeSkill: 'swizzle_prepare' }, { activeSkill: 'xgoduck' }]) {
      expect(rollerResumeReady({ ...control, ...patch })).toBe(false);
    }
    expect(rollerResumeReady(undefined)).toBe(false);
  });
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
