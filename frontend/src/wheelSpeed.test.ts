import { describe,it,expect } from 'vitest';
import { wheelTwist } from './wheelSpeed';
describe('wheel speed commands',()=>{
  it('sends selected forward speed with proportional drag and correct signs',()=>{
    expect(wheelTwist(0,-1,.2)).toEqual([.2,0,-0]);
    expect(wheelTwist(0,-.5,.2)[0]).toBe(.1);
    expect(wheelTwist(0,1,.15)[0]).toBe(-.15);
    expect(wheelTwist(0,-1,.05)[0]).toBe(.05);
  });
  it('turns both directions and combines forward/yaw within public limits',()=>{
    expect(wheelTwist(1,0,.2)).toEqual([-0,0,-.5]);
    expect(wheelTwist(-1,0,.2)).toEqual([-0,0,.5]);
    const [vx,vy,wz]=wheelTwist(1,-1,.2);expect(vx).toBeCloseTo(.2/Math.sqrt(2));expect(vy).toBe(0);expect(wz).toBeCloseTo(-.5/Math.sqrt(2));
    expect(wheelTwist(0,-1,1)[0]).toBe(.2);
  });
  it('zeroes deadzone and invalid inputs',()=>{
    expect(wheelTwist(.05,.05,.2)).toEqual([0,0,0]);
    expect(wheelTwist(0,-1,NaN)).toEqual([0,0,0]);
  });
});
