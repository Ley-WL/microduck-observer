import { afterEach, describe, expect, it, vi } from 'vitest';
import { modelUpload, registryIdle, registryRequest, slotModels, type ModelRegistry } from './modelRegistry';
afterEach(() => vi.unstubAllGlobals());
describe('model registry', () => {
  it('only permits changes in known idle states', () => {
    for (const state of ['policy', 'preflight', 'moving', 'disabling', undefined, 'unknown']) expect(registryIdle(state)).toBe(false);
    for (const state of ['idle', 'holding', 'disabled', 'failed']) expect(registryIdle(state)).toBe(true);
  });
  it('keeps candidates in their own slot including unavailable current models', () => {
    const registry = { models: [{ id: 'a', slot: 'walk', available: false }, { id: 'b', slot: 'flamingo', available: true }] } as ModelRegistry;
    expect(slotModels(registry, 'walk').map(m => m.id)).toEqual(['a']);
  });
  it('sends explicit selections with revision and surfaces rejected contracts', async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ detail: '模型合同不匹配' }), { status: 409 }));
    vi.stubGlobal('fetch', fetcher);
    await expect(registryRequest('http://robot/', 'select', { revision: 4, slot: 'walk', id: 'candidate' })).rejects.toThrow('模型合同不匹配');
    expect(fetcher.mock.calls[0]?.[0]).toBe('http://robot/api/v1/model-registry/select');
    expect(JSON.parse(fetcher.mock.calls[0]?.[1].body)).toEqual({ revision: 4, slot: 'walk', id: 'candidate' });
  });
  it('uploads exact model bytes and metadata, never activates on import', async () => {
    const onnx = new File([new Uint8Array([0, 128, 255])], 'candidate.onnx');
    expect(await modelUpload(onnx)).toEqual({ onnxBase64: 'AID/' });
    const metadata = new File(['{"policySha256":"abc"}'], 'candidate.metadata.json');
    expect(await modelUpload(onnx, metadata)).toEqual({ onnxBase64: 'AID/', metadata: { policySha256: 'abc' } });
    await expect(modelUpload(new File([], 'empty.onnx'), metadata)).rejects.toThrow('ONNX');
    await expect(modelUpload(onnx, new File(['[]'], 'bad.json'))).rejects.toThrow('JSON对象');
  });
});
