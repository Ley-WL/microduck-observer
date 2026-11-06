import { afterEach, expect, it, vi } from 'vitest';
import { shortLongPress } from './shortLongPress';
afterEach(()=>vi.useRealTimers());
it('short press fires only at release, long press never falls through to short',()=>{
 vi.useFakeTimers();const short=vi.fn(),long=vi.fn(),p=shortLongPress(short,long);
 p.start();vi.advanceTimersByTime(699);expect(short).not.toHaveBeenCalled();p.release();expect(short).toHaveBeenCalledTimes(1);expect(long).not.toHaveBeenCalled();
 p.start();vi.advanceTimersByTime(700);expect(long).toHaveBeenCalledTimes(1);p.release();expect(short).toHaveBeenCalledTimes(1);
});
it('cancel, repeated starts and late release cannot dispatch stale gestures',()=>{
 vi.useFakeTimers();const short=vi.fn(),long=vi.fn(),p=shortLongPress(short,long);
 p.start();p.start();p.cancel();vi.advanceTimersByTime(1000);p.release();expect(short).not.toHaveBeenCalled();expect(long).not.toHaveBeenCalled();
 p.start();vi.advanceTimersByTime(1000);p.release();p.release();expect(long).toHaveBeenCalledTimes(1);expect(short).not.toHaveBeenCalled();
});
