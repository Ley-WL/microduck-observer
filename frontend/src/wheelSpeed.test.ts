import { describe,it,expect } from 'vitest';
import { gamepadSpeed,wheelTwist,rollerTwist } from './wheelSpeed';
describe('wheel speed commands',()=>{
  it('uses 0.2 m/s by default and LT doubles it up to 0.4 m/s',()=>{
    expect(gamepadSpeed(.2,false)).toBe(.2);
    expect(gamepadSpeed(.2,true)).toBe(.4);
    expect(gamepadSpeed(.3,true)).toBe(.4);
  });
  it('sends selected forward speed with proportional drag and correct signs',()=>{
    expect(wheelTwist(0,-1,.2)).toEqual([.2,0,-0]);
    expect(wheelTwist(0,-.5,.2)[0]).toBe(.1);
    expect(wheelTwist(0,1,.15)[0]).toBe(-.15);
    expect(wheelTwist(0,1,.4)[0]).toBe(-.4);
    expect(wheelTwist(0,-1,.05)[0]).toBe(.05);
  });
  it('turns both directions and combines forward/yaw within public limits',()=>{
    expect(wheelTwist(1,0,.2)).toEqual([-0,0,-1]);
    expect(wheelTwist(-1,0,.2)).toEqual([-0,0,1]);
    const [vx,vy,wz]=wheelTwist(1,-1,.2);expect(vx).toBeCloseTo(.2/Math.sqrt(2));expect(vy).toBe(0);expect(wz).toBeCloseTo(-1/Math.sqrt(2));
    expect(wheelTwist(0,-1,1)[0]).toBe(.4);
  });
  it('zeroes deadzone and invalid inputs',()=>{
    expect(wheelTwist(.05,.05,.2)).toEqual([0,0,0]);
    expect(wheelTwist(0,-1,NaN)).toEqual([0,0,0]);
  });
});

it('roller throttle and heading stay bounded with centered and invalid inputs zeroed',()=>{
  expect(rollerTwist(0,-1)).toEqual([.4,0,-0]);expect(rollerTwist(0,1)[0]).toBe(-.4);
  expect(rollerTwist(1,0)[2]).toBe(-.25);expect(rollerTwist(-1,0)[2]).toBe(.25);
  for(const [x,y] of [[.01,.01],[NaN,0],[0,Infinity]]) expect(rollerTwist(x!,y!)).toEqual([0,0,0]);
  const v=rollerTwist(1,1);expect(Math.abs(v[0])).toBeLessThan(.4);expect(Math.abs(v[2])).toBeLessThan(.25);
});
