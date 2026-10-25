export const modelSlots = [
  { id: 'walk', label: '行走' }, { id: 'sitstand', label: '坐下 / 站起' },
  { id: 'getup', label: '倒地起身' }, { id: 'pick', label: '拾取' },
  { id: 'roulade', label: '翻滚' }, { id: 'kick_left', label: '左脚踢球' },
  { id: 'kick_right', label: '右脚踢球' }, { id: 'dance', label: '舞蹈' },
  { id: 'flamingo', label: '金鸡独立' }, { id: 'swizzle', label: '轮滑' },
] as const;
export interface RegisteredModel {
  id: string; label: string; slot: string; file: string; imported?: boolean;
  available: boolean; error?: string;
  metadata?: { runtimeKind?: string; policySha256?: string; hardwareValidated?: boolean; swingFoot?: string };
}
export interface CustomAction { id: string; label: string; templateSlot: string }
export interface ModelRegistry {
  actions?: CustomAction[];
  presets: { id: string; label: string; slot: string; metadata: Record<string, unknown> }[];
  schemaVersion: number; revision: number; models: RegisteredModel[];
  selected: Record<string, string>; previous: Record<string, string>;
}
export function registryIdle(state: string | undefined): boolean {
  return ['idle', 'disabled', 'holding', 'failed'].includes(state ?? '');
}
export function slotModels(registry: ModelRegistry, slot: string): RegisteredModel[] {
  return registry.models.filter(model => model.slot === slot);
}
export async function registryRequest(endpoint: string, action = '', body?: unknown): Promise<any> {
  const response = await fetch(endpoint.replace(/\/$/, '') + '/api/v1/model-registry' + (action ? '/' + action : ''), {
    method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(action === 'import' ? 120000 : 30000),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.detail || '模型管理请求失败');
  return result;
}
export async function modelUpload(onnx: File, metadata?: File): Promise<{ onnxBase64: string; metadata?: unknown }> {
  if (!onnx.name.endsWith('.onnx') || onnx.size === 0 || onnx.size > 16 * 1024 * 1024) throw new Error('请选择不超过16MiB的ONNX文件');
  if (metadata && metadata.size > 256 * 1024) throw new Error('metadata文件不能超过256KiB');
  const parsed = metadata ? JSON.parse(await metadata.text()) : undefined;
  if (metadata && (!parsed || typeof parsed !== 'object' || Array.isArray(parsed))) throw new Error('metadata必须是JSON对象');
  const bytes = new Uint8Array(await onnx.arrayBuffer());
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += 8192) binary += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
  return { onnxBase64: btoa(binary), ...(metadata ? { metadata: parsed } : {}) };
}
