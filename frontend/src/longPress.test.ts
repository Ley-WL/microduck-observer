import {describe,it,expect,vi,afterEach} from 'vitest';
import {longPress} from './longPress';
afterEach(()=>vi.useRealTimers());
describe('autonomy long press',()=>{
 it('fires at 1.5 seconds once, including repeated keyboard down events',()=>{vi.useFakeTimers();const fire=vi.fn();const hold=longPress(fire);hold.start();vi.advanceTimersByTime(1499);expect(fire).not.toHaveBeenCalled();hold.start();vi.advanceTimersByTime(1);expect(fire).toHaveBeenCalledTimes(1);vi.advanceTimersByTime(5000);hold.start();vi.advanceTimersByTime(2000);expect(fire).toHaveBeenCalledTimes(1);hold.cancel();hold.start();vi.advanceTimersByTime(1500);expect(fire).toHaveBeenCalledTimes(2);});
 it('release, blur or cancellation before the threshold does not toggle',()=>{vi.useFakeTimers();const fire=vi.fn();const hold=longPress(fire);hold.start();vi.advanceTimersByTime(1400);hold.cancel();vi.advanceTimersByTime(2000);expect(fire).not.toHaveBeenCalled();hold.start();vi.advanceTimersByTime(100);hold.cancel();expect(fire).not.toHaveBeenCalled();});
});
