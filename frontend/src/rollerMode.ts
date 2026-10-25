import type { Bindings } from './padBindings';

/** Mode-local overrides never alter the user's saved ordinary-action bindings. */
export function rollerBindings(bindings: Bindings | undefined, roller: boolean): Bindings | undefined {
  return roller ? { ...bindings, a: 'swizzle', x: 'roller_crouch' } : bindings;
}
