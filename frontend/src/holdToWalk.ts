export type Twist = [number, number, number];
/** Each press renews a short server lease. Releases never wait for prior requests. */
export function holdToWalk(send: (twist: Twist, sequence: number) => Promise<void>) {
  let timer: ReturnType<typeof setInterval> | undefined;
  let sequence = 0;
  let held = false;
  function release() {
    if (timer) clearInterval(timer);
    timer = undefined;
    if (!held) return;
    held = false;
    void send([0, 0, 0], ++sequence).catch(() => {});
  }
  function press(twist: Twist) {
    release();
    held = true;
    const renew = () => { void send(twist, ++sequence).catch(release); };
    renew();
    timer = setInterval(renew, 80);
  }
  return { press, release };
}
