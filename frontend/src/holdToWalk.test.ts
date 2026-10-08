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

it('skills and mouth commands share the monotonic drive sequence after release', () => {
  vi.useFakeTimers();
  const send = vi.fn(async () => {}), hold = holdToWalk(send);
  hold.press([.2,0,0]);hold.release();
  expect(hold.nextSequence()).toBe(3);
  hold.press([0,0,.5]);
  expect(send.mock.calls.at(-1)).toEqual([[0,0,.5],4]);
  hold.release();
});

it('wheel updates renew the latest vector without intermediate zero and release stops it', () => {
  vi.useFakeTimers();
  const send=vi.fn(async()=>{}),hold=holdToWalk(send);
  hold.press([.1,0,0]);hold.update([.05,.05,0]);vi.advanceTimersByTime(80);
  expect(send.mock.calls.at(-1)).toEqual([[.05,.05,0],2]);
  hold.release();vi.advanceTimersByTime(160);
  expect(send.mock.calls.at(-1)).toEqual([[0,0,0],3]);expect(send).toHaveBeenCalledTimes(3);
});

it('surfaces a rejected movement, sends zero and stops renewals',async()=>{
  vi.useFakeTimers();const error=new Error('会话过期'),onError=vi.fn();
  const send=vi.fn().mockRejectedValueOnce(error).mockResolvedValue(undefined);
  const hold=holdToWalk(send,{onError});hold.press([.1,0,0]);await Promise.resolve();await Promise.resolve();
  expect(onError).toHaveBeenCalledWith(error);expect(send.mock.calls.at(-1)?.[0]).toEqual([0,0,0]);
  vi.advanceTimersByTime(240);expect(send).toHaveBeenCalledTimes(2);
});
it('a delayed rejection from an old press cannot cancel a new press',async()=>{
  vi.useFakeTimers();let reject!:(reason:Error)=>void;
  const send=vi.fn().mockImplementationOnce(()=>new Promise<void>((_,r)=>{reject=r;})).mockResolvedValue(undefined);
  const onError=vi.fn(),hold=holdToWalk(send,{onError});hold.press([.1,0,0]);hold.press([0,0,.5]);
  reject(new Error('old request'));await Promise.resolve();vi.advanceTimersByTime(80);
  expect(send.mock.calls.at(-1)?.[0]).toEqual([0,0,.5]);expect(onError).not.toHaveBeenCalled();hold.release();
});
