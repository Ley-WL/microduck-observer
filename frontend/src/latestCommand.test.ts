import { expect, test } from 'vitest';
import { latestCommand } from './latestCommand';
const flush = async () => { await Promise.resolve(); await Promise.resolve(); };
test('drag dispatches immediately and retains only latest sample including release', async () => {
  const sent: number[] = []; const finishes: (() => void)[] = [];
  const queue = latestCommand<number>(n => { sent.push(n); return new Promise<void>(r => finishes.push(r)); });
  queue.push(1); queue.push(2); queue.push(3);
  expect(sent).toEqual([1]); finishes.shift()!(); await flush(); expect(sent).toEqual([1,3]);
  finishes.shift()!(); await flush();
});
test('clear discards trailing command while request is in flight', async () => {
  const sent: number[] = []; let finish!: () => void;
  const queue = latestCommand<number>(n => { sent.push(n); return new Promise<void>(r => finish=r); });
  queue.push(1); queue.push(2); queue.clear(); finish(); await flush(); expect(sent).toEqual([1]);
});
test('failed request discards queued targets and accepts a new gesture', async () => {
  const sent: number[] = []; let reject!: () => void;
  const queue = latestCommand<number>(n => { sent.push(n); return n===1 ? new Promise<void>((_,r) => reject=()=>r(new Error('offline'))) : Promise.resolve(); });
  queue.push(1); queue.push(2); reject(); await flush(); expect(sent).toEqual([1]);
  queue.push(3); await flush(); expect(sent).toEqual([1,3]);
});
