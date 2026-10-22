import { afterEach, expect, it, vi } from 'vitest';
import { bindingLabel, padButtons, requestBindings } from './padBindings';
afterEach(() => vi.unstubAllGlobals());
it('labels follow the mapping and missing configuration never implies a default action', () => {
  expect(bindingLabel({ x: 'kick_left' }, 'x')).toBe('左脚踢球');
  expect(bindingLabel({ x: 'none' }, 'x')).toBe('不绑定');
  expect(bindingLabel({ x: 'custom-wave' }, 'x', { 'custom-wave': '招手' })).toBe('招手');
  expect(bindingLabel(undefined, 'x')).toBe('配置未读取');
  for (const key of ['back', 'b', 'start', 'lt', 'rt', 'down']) expect(padButtons).not.toContain(key);
});
it('saves exact bindings with revision and surfaces conflicts without sending motion', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ detail: '请先停止模型' }), { status: 409 }));
  vi.stubGlobal('fetch', fetcher);
  await expect(requestBindings('http://robot/', { revision: 3, bindings: { x: 'dance' } })).rejects.toThrow('请先停止模型');
  expect(fetcher.mock.calls[0]?.[0]).toBe('http://robot/api/v1/gamepad/bindings');
  expect(JSON.parse(fetcher.mock.calls[0]?.[1].body)).toEqual({ revision: 3, bindings: { x: 'dance' } });
});
