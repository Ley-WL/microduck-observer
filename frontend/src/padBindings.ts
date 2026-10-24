export const padButtons = ['a', 'x', 'y', 'lb', 'rb', 'up', 'left', 'right'] as const;
export type PadButton = typeof padButtons[number];
export const actionLabels: Record<string, string> = { none: '不绑定', standup: '站立', sit: '坐下', getup: '倒地起身', demo: '演示', pick: '拾取', roulade: '翻滚', kick_left: '左脚踢球', kick_right: '右脚踢球', dance: '舞蹈', flamingo: '抬脚 / 落脚', quack: '嘎一下' };
export const buttonLabels: Record<string, string> = { a: 'A', x: 'X', y: 'Y', lb: 'LB', rb: 'RB', up: '十字键 ↑', left: '十字键 ←', right: '十字键 →' };
export type Bindings = Record<string, string>;
export interface PadConfig { schemaVersion: number; revision: number; bindings: Bindings }
export interface PadBindingInfo { config: PadConfig; defaults: Bindings; buttons: PadButton[]; actions: string[]; customActions?: { id: string; label: string; templateSlot: string }[] }
export function bindingLabel(bindings: Bindings | undefined, key: string, labels: Record<string, string> = {}): string {
  return bindings ? labels[bindings[key] ?? 'none'] ?? actionLabels[bindings[key] ?? 'none'] ?? '未知动作' : '配置未读取';
}
export async function requestBindings(endpoint: string, body?: unknown): Promise<any> {
  const response = await fetch(endpoint.replace(/\/$/, '') + '/api/v1/gamepad/bindings', {
    method: body === undefined ? 'GET' : 'POST', headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(10000),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.detail || '手柄配置请求失败');
  return result;
}
