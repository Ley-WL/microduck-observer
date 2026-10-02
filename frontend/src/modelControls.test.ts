import { test, expect } from 'vitest';
import { keyboardMove, joystickMove, idleMove, POLICY_OPTIONS, SKILLS } from './modelControls';
test('keys give correct trunk direction and opposing keys cancel', () => {
  expect(keyboardMove(new Set(['w','a','q']))).toEqual({forward:1,lateral:1,turn:1});
  expect(keyboardMove(new Set(['s','d','e']))).toEqual({forward:-1,lateral:-1,turn:-1});
  expect(keyboardMove(new Set(['w','s','a','d','q','e']))).toEqual(idleMove());
  expect(keyboardMove(new Set())).toEqual(idleMove());
});
test('joystick forward and left match keys; pointer outside stays bounded', () => {
  expect(joystickMove(0,-1)).toEqual(keyboardMove(new Set(['w'])));
  expect(joystickMove(-1,0)).toEqual(keyboardMove(new Set(['a'])));
  const move=joystickMove(2,-2);
  expect(Math.hypot(move.forward,move.lateral)).toBeCloseTo(1);
  expect(joystickMove(NaN,0)).toEqual(idleMove());
});
test('model entries distinguish gait selection and one-shot skills', () => {
  expect(POLICY_OPTIONS.find(p=>p.file==='velstand.onnx')?.kind).toBe('walk');
  expect(SKILLS.map(p=>p.file)).toEqual(['alpha_ground_pick.onnx','ball_kick_left.onnx','ball_kick_right.onnx','roulade.onnx']);
});
