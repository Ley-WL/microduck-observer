import { afterEach, expect, it, vi } from 'vitest';
import { parseTofSettings, requestTof } from './tofSettings';
const settings = { available: true, enabled: false, streaming: false, state: 'disabled', error: '', device: '/dev/i2c-3', sensor: 'VL53L5CX', targetHz: 15 };
afterEach(() => vi.unstubAllGlobals());
it('distinguishes off, starting and streaming states and rejects malformed status', () => {
  expect(parseTofSettings(settings).enabled).toBe(false);
  expect(parseTofSettings({ ...settings, enabled: true, state: 'starting' }).streaming).toBe(false);
  expect(() => parseTofSettings({ ...settings, enabled: 'false' })).toThrow();
  expect(() => parseTofSettings({ ...settings, targetHz: NaN })).toThrow();
});
it('sends only an explicit switch and surfaces rejected writes', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(settings)));
  vi.stubGlobal('fetch', fetcher);
  await requestTof('http://duck/', false);
  expect(fetcher.mock.calls[0]?.[0]).toBe('http://duck/api/v1/tof');
  expect(fetcher.mock.calls[0]?.[1].body).toBe('{"enabled":false}');
  fetcher.mockResolvedValue(new Response(JSON.stringify({ detail: 'I2C不可用' }), { status: 503 }));
  await expect(requestTof('http://duck', true)).rejects.toThrow('I2C不可用');
});
