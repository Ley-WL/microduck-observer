import { describe, expect, it } from 'vitest';
import { validTof, tofStats } from './tof';
describe('ToF hardware telemetry', () => {
  it('rejects malformed grids and non-finite telemetry', () => {
    const frame = { rows: 8, cols: 8, distanceMm: Array(64).fill(358), status: Array(64).fill(5), hz: 13.5 };
    expect(validTof(frame)).toBe(true);
    expect(validTof({...frame, status: [5]})).toBe(false);
    expect(validTof({...frame, distanceMm: Array(64).fill(NaN)})).toBe(false);
    expect(validTof({...frame, hz: Infinity})).toBe(false);
  });
  it('excludes invalid zones without excluding a valid zero', () => {
    expect(tofStats({distanceMm:[0,350,400,9999], status:[5,5,5,0]})).toEqual({count:3,min:0,max:400,median:350});
    expect(tofStats({distanceMm:[9999],status:[0]})).toEqual({count:0,min:null,max:null,median:null});
  });
});
