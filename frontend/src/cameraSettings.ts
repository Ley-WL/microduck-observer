export type CameraSettings = {
  enabled: boolean; streaming: boolean; error: string; device: string;
  autonomousUse?: boolean;
  autonomousManaged?: boolean;
  captureEnabled?: boolean;
  transport: 'mjpeg'; width: number; height: number; targetFps: number; sequence: number;
};
export function cameraBase(endpoint: string): string {
  const url = new URL(endpoint);
  url.pathname = '/api/v1/camera'; url.search = ''; url.hash = '';
  return url.origin + url.pathname;
}
export function parseCameraSettings(value: unknown): CameraSettings {
  const v = value as CameraSettings;
  if (!v || typeof v.enabled !== 'boolean' || typeof v.streaming !== 'boolean'
      || typeof v.error !== 'string' || typeof v.device !== 'string' || v.transport !== 'mjpeg'
      || ![v.width, v.height, v.targetFps, v.sequence].every(n => Number.isFinite(n) && n >= 0)) {
    throw new Error('摄像头状态响应无效');
  }
  return v;
}
