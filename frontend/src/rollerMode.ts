import type { Bindings } from './padBindings';

export function rollerResumeReady(control: Record<string, unknown> | undefined): boolean {
  return control?.state === 'policy' && control.policyPhase === 'roller-fallen-wait'
    && control.rollerResumeReady === true && ['swizzle', 'swizzle_crouch'].includes(String(control.activeSkill));
}
export function rollerCrouchReturnReady(control: Record<string, unknown> | undefined): boolean {
  return control?.state === 'policy' && control.activeSkill === 'swizzle_crouch'
    && control.feedbackHolding !== true && control.rollerCrouchReturning !== true;
}

/** Mode-local overrides never alter the user's saved ordinary-action bindings. */
export function rollerBindings(bindings: Bindings | undefined, roller: boolean): Bindings | undefined {
  return roller ? { ...bindings, a: 'swizzle', x: 'roller_crouch' } : bindings;
}
