import { describe, expect, it } from 'vitest';
import { validTof, tofStats, tofColumns } from './tof';
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

it('radar selects valid minima per column, retains source zones and empty columns', () => {
 const cells = Array.from({length:64}, () => ({distance:undefined as number|undefined,valid:false}));
 cells[0]={distance:999,valid:true}; cells[8]={distance:350,valid:true};
 cells[16]={distance:1,valid:false}; cells[1]={distance:0,valid:true};
 cells[2]={distance:4500,valid:true};
 const columns=tofColumns(cells);
 expect(columns[0]).toEqual({index:8,column:0,distance:350});
 expect(columns[1]?.distance).toBe(0);
 expect(columns[2]?.distance).toBe(4500);
 expect(columns[3]).toBeNull();
 expect(tofColumns([]).every(x=>x===null)).toBe(true);
});
