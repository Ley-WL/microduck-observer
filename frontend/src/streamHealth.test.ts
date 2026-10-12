import { describe, expect, it } from "vitest";
import { dashboardTopics, StreamWatchdog, streamCloseReason } from "./streamHealth";

describe("stream health", () => {
  it("keeps normal foreground silence detection", () => {
    const w = new StreamWatchdog();w.open(0);
    for(let t=100;t<=6000;t+=100) expect(w.expired(t,false)).toBe(false);
    expect(w.expired(6100,false)).toBe(true);
    w.message(6100);expect(w.expired(6200,false)).toBe(false);
  });
  it("allows buffered messages to drain after an event-loop suspension", () => {
    const w=new StreamWatchdog();w.open(0);
    expect(w.expired(15000,false)).toBe(false);
    w.message(15020);
    for(let t=15100;t<21000;t+=100)expect(w.expired(t,false)).toBe(false);
    expect(w.expired(21100,false)).toBe(true);
  });
  it("does not reconnect a hidden page and restores a bounded foreground window", () => {
    const w=new StreamWatchdog();w.open(0);
    for(let t=100;t<=20000;t+=100)expect(w.expired(t,true)).toBe(false);
    expect(w.expired(20100,false)).toBe(false);
    for(let t=20200;t<26100;t+=100)expect(w.expired(t,false)).toBe(false);
    expect(w.expired(26100,false)).toBe(true);
  });
  it("retains orientation rate while reducing large diagnostic streams", () => {
    expect(dashboardTopics["imu.orientation"]).toBe(50);
    expect(dashboardTopics.joints).toBe(10);expect(dashboardTopics["imu.raw"]).toBe(10);
    expect(dashboardTopics.logs).toBeNull();expect(dashboardTopics.system).toBe(1);
  });
  it("preserves useful close diagnostics", () => {
    expect(streamCloseReason(1006,"")).toContain("1006");
    expect(streamCloseReason(1008,"Invalid subscription")).toContain("Invalid subscription");
    expect(streamCloseReason(4000,"heartbeat timeout")).toContain("心跳超时");
  });
});
