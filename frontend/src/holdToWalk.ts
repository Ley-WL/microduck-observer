export type Twist = [number, number, number];
/** Each press renews a short server lease. Releases never wait for prior requests. */
export function holdToWalk(send: (twist: Twist, sequence: number) => Promise<void>, options: {nextSequence?:()=>number;onError?:(error:unknown)=>void} = {}) {
  let timer: ReturnType<typeof setInterval> | undefined;
  let sequence = 0;
  let held = false;
  let current: Twist = [0,0,0];
  let generation=0;
  const next=()=>options.nextSequence?options.nextSequence():++sequence;
  function release() {
    if (timer) clearInterval(timer);
    timer = undefined;
    if (!held) return;
    held = false;
    generation++;
    void send([0, 0, 0], next()).catch(() => {});
  }
  function press(twist: Twist) {
    release();
    held = true;
    current = twist;
    const id=++generation;
    const renew = () => { void send(current, next()).catch(error=>{if(id!==generation)return;release();options.onError?.(error);}); };
    renew();
    timer = setInterval(renew, 80);
  }
  return { press, release, update: (twist: Twist) => { current = twist; }, nextSequence: next };
}
