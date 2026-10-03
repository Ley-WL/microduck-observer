import { expect, it } from 'vitest';
import { nextTick, reactive } from 'vue';
import { watchControlIdentity } from './controlIdentity';
import { stepAngle } from './servoSelection';

it('keeps accumulated clicks across same-revision calibration polls and joint frames', async () => {
  const state = reactive({ endpoint: 'http://board', joints: { bootId: 'boot', position: 2048 }, calibration: { revision: 133 } });
  let draft: number | undefined;
  const stop = watchControlIdentity([
    () => state.endpoint, () => state.joints.bootId, () => state.calibration.revision,
  ], () => { draft = undefined; });
  for (let i = 0; i < 5; i++) {
    draft = stepAngle(draft ?? 0, 1, [-65, 65])!;
    state.calibration = { revision: 133 };
    state.joints = { bootId: 'boot', position: 2048 };
    await nextTick();
  }
  expect(draft).toBe(5);
  state.calibration = { revision: 134 };
  await nextTick();
  expect(draft).toBeUndefined();
  draft = 2;
  state.joints = { bootId: 'new-boot', position: 2048 };
  await nextTick();
  expect(draft).toBeUndefined();
  draft = 3;
  state.endpoint = 'http://other-board';
  await nextTick();
  expect(draft).toBeUndefined();
  stop();
});
