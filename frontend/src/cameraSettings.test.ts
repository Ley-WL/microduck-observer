import { describe, expect, it } from 'vitest';
import { cameraBase, parseCameraSettings } from './cameraSettings';
describe('camera settings', () => {
  const state = { enabled: false, streaming: false, error: '', device: '', transport: 'mjpeg', width: 640, height: 360, targetFps: 10, sequence: 0 };
  it('uses the connected robot host, clearing unrelated paths', () => {
    expect(cameraBase('http://192.168.137.74:8877/view?x=1#test')).toBe('http://192.168.137.74:8877/api/v1/camera');
  });
  it('keeps persisted enablement distinct from actual frames', () => {
    expect(parseCameraSettings({ ...state, enabled: true, error: 'sensor missing' }).streaming).toBe(false);
  });
  it('rejects malformed states instead of interpreting truthy strings as enabled', () => {
    for (const value of [null, {}, { ...state, enabled: 'false' }, { ...state, sequence: NaN }]) {
      expect(() => parseCameraSettings(value)).toThrow();
    }
  });
});
