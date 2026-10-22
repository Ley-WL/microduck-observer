export interface TofSettings {
  available: boolean; enabled: boolean; streaming: boolean; state: string;
  error: string; device: string; sensor: string; targetHz: number;
}
export function parseTofSettings(v: any): TofSettings {
  if (!v || !['available','enabled','streaming'].every(k => typeof v[k] === 'boolean')
    || !['state','error','device','sensor'].every(k => typeof v[k] === 'string')
    || !Number.isFinite(v.targetHz) || v.targetHz <= 0) throw new Error('ToF 状态响应无效');
  return v;
}
export async function requestTof(endpoint: string, enabled?: boolean): Promise<TofSettings> {
  const response = await fetch(endpoint.replace(/\/$/, '') + '/api/v1/tof', {
    method: enabled === undefined ? 'GET' : 'POST',
    headers: enabled === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: enabled === undefined ? undefined : JSON.stringify({ enabled }), signal: AbortSignal.timeout(15000),
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.detail || 'ToF 设置请求失败');
  return parseTofSettings(value);
}
