// One request in flight; intermediate drag samples are replaced, never queued.
export function latestCommand<T>(send: (value: T) => Promise<void>) {
  let next: T | undefined, running = false;
  async function drain() {
    if (running) return;
    running = true;
    try {
      while (next !== undefined) {
        const value = next; next = undefined;
        await send(value);
      }
    } catch { next = undefined; }
    finally { running = false; }
  }
  return {
    push(value: T) { next = value; void drain(); },
    clear() { next = undefined; },
  };
}
