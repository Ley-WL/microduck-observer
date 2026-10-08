import { afterEach, expect, it, vi } from 'vitest';
import { holdToWalk } from './holdToWalk';
afterEach(() => vi.useRealTimers());
it('release immediately sends zero despite an unresolved press and stops renewal', () => {
  vi.useFakeTimers();
  const send = vi.fn(() => new Promise<void>(() => {}));
  const hold = holdToWalk(send);
  hold.press([.2, 0, 0]);
  vi.advanceTimersByTime(160);
  hold.release();
  expect(send.mock.calls.at(-1)).toEqual([[0, 0, 0], 4]);
  vi.advanceTimersByTime(500);
  expect(send).toHaveBeenCalledTimes(4);
});
it('switching direction zeros the old press before a new one', () => {
  vi.useFakeTimers();
  const send = vi.fn(async () => {});
  const hold = holdToWalk(send);
  hold.press([.2, 0, 0]); hold.press([0, 0, .5]); hold.release();
  expect(send.mock.calls).toEqual([[[.2, 0, 0], 1], [[0, 0, 0], 2], [[0, 0, .5], 3], [[0, 0, 0], 4]]);
});
