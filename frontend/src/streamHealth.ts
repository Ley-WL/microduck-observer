// Display rates only: hardware acquisition and policy loops keep their own rates.
export const dashboardTopics = {
  pose: 50, "imu.orientation": 50, "imu.raw": 10,
  joints: 10, tof: 10, gamepad: 25, system: 1, logs: null,
};

export class StreamWatchdog {
  private lastMessage = 0;
  private lastTick = 0;
  private graceUntil = 0;
  private wasHidden = false;
  open(now: number) { this.lastMessage = this.lastTick = now; this.graceUntil = 0; this.wasHidden = false; }
  message(now: number) { this.lastMessage = now; }
  expired(now: number, hidden: boolean): boolean {
    // A resumed event loop can run its overdue timer before queued socket messages.
    // Give those messages a foreground heartbeat window; sample ages remain unchanged.
    if (now - this.lastTick > 2000 || (this.wasHidden && !hidden)) this.graceUntil = now + 6000;
    this.lastTick = now;
    this.wasHidden = hidden;
    return !hidden && now >= this.graceUntil && now - this.lastMessage > 6000;
  }
}

export function streamCloseReason(code: number, reason: string): string {
  if (code === 4000) return "服务心跳超时，正在自动重试";
  return `连接断开（${code}${reason ? `：${reason}` : ""}），正在自动重试`;
}
