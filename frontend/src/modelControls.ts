export const POLICY_OPTIONS = [
  { file: 'alpha_stand.onnx', name: '站立', hint: '持续站立', kind: 'stand' },
  { file: 'velstand.onnx', name: '站立 / 行走', hint: 'v5 默认步行策略', kind: 'walk' },
  { file: 'alpha_walking.onnx', name: '步行', hint: '备用步行策略', kind: 'walk' },
  { file: 'alpha_sitstand.onnx', name: '坐下 / 起立', hint: '坐姿与站姿转换', kind: 'posture' },
] as const;
export const SKILLS = [
  { file: 'alpha_ground_pick.onnx', name: '低头拾取', icon: '↘', hint: '相位动作' },
  { file: 'ball_kick_left.onnx', name: '左脚踢球', icon: '↖', hint: '单次动作' },
  { file: 'ball_kick_right.onnx', name: '右脚踢球', icon: '↗', hint: '单次动作' },
  { file: 'roulade.onnx', name: '翻滚', icon: '⟳', hint: '连续技能' },
] as const;
export type Move = { forward: number; lateral: number; turn: number };
export const idleMove = (): Move => ({ forward: 0, lateral: 0, turn: 0 });
export function keyboardMove(keys: ReadonlySet<string>): Move {
  return { forward: Number(keys.has('w')) - Number(keys.has('s')),
    lateral: Number(keys.has('a')) - Number(keys.has('d')),
    turn: Number(keys.has('q')) - Number(keys.has('e')) };
}
export function joystickMove(x: number, y: number): Move {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return idleMove();
  const length = Math.max(1, Math.hypot(x, y));
  return { forward: (-y / length) || 0, lateral: (-x / length) || 0, turn: 0 };
}
export function editableTarget(target: EventTarget | null) {
  return target instanceof HTMLElement && !!target.closest('input, textarea, select, button, [contenteditable="true"], dialog');
}
