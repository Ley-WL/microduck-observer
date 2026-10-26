import {afterEach,expect,it,vi} from 'vitest';
import {heldRoll} from './heldRoll';
afterEach(()=>vi.useRealTimers());
it('refreshes within the chain window and stops on release without an extra request',()=>{
  vi.useFakeTimers();const fire=vi.fn();const h=heldRoll(fire,()=>true);
  h.start();h.start();expect(fire).toHaveBeenCalledTimes(1);
  vi.advanceTimersByTime(180);expect(fire).toHaveBeenCalledTimes(4);
  h.cancel();vi.advanceTimersByTime(1000);expect(fire).toHaveBeenCalledTimes(4);
  h.start();expect(fire).toHaveBeenCalledTimes(5);h.cancel();
});
it('cannot retain a held request across a disabled or disconnected state',()=>{
  vi.useFakeTimers();let enabled=false;const fire=vi.fn();const h=heldRoll(fire,()=>enabled);
  h.start();expect(fire).not.toHaveBeenCalled();enabled=true;h.start();enabled=false;
  vi.advanceTimersByTime(60);enabled=true;vi.advanceTimersByTime(500);expect(fire).toHaveBeenCalledTimes(1);
});
