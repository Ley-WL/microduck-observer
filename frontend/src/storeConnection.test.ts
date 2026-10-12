import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useTelemetry } from "./store";

class Socket {
  static OPEN=1;
  static all: Socket[]=[];
  readyState=0;
  onopen?: () => void;
  onmessage?: (e: {data: string}) => void;
  onclose?: (e: {code: number;reason: string}) => void;
  onerror?: () => void;
  send=vi.fn();
  constructor(..._args: unknown[]){Socket.all.push(this);}
  close(code=1000,reason=""){this.readyState=3;this.onclose?.({code,reason});}
  open(){this.readyState=1;this.onopen?.();}
  subscribed(){this.onmessage?.({data:JSON.stringify({type:"subscribed",protocolVersion:1})});}
}

describe("dashboard reconnection ownership",()=>{
  let state: ReturnType<typeof useTelemetry>;
  beforeEach(()=>{
    vi.useFakeTimers();Socket.all=[];
    vi.stubGlobal("WebSocket",Socket);
    vi.stubGlobal("window",{location:{origin:"http://localhost:8877"}});
    vi.stubGlobal("document",{hidden:false});
    vi.stubGlobal("localStorage",{getItem:()=>null,setItem:vi.fn()});
    vi.stubGlobal("fetch",vi.fn(async()=>({ok:true,json:async()=>({protocolVersion:1,bootId:"fixture",source:"hardware"})})));
    vi.spyOn(Math,"random").mockReturnValue(0);
    setActivePinia(createPinia());state=useTelemetry();
  });
  afterEach(()=>{state.dispose();vi.restoreAllMocks();vi.unstubAllGlobals();vi.useRealTimers();});
  it("ignores late close/messages from an older automatic attempt",async()=>{
    await state.connect();const old=Socket.all[0];old.open();old.subscribed();expect(state.connection).toBe("在线");
    old.close(1006);await vi.advanceTimersByTimeAsync(700);
    const current=Socket.all[1];expect(current).toBeDefined();current.open();current.subscribed();
    old.close(1008,"late old event");old.onmessage?.({data:'{"type":"subscribed","protocolVersion":99}'});
    expect(state.connection).toBe("在线");await vi.advanceTimersByTimeAsync(1500);expect(Socket.all).toHaveLength(2);
  });
  it("retries a stalled websocket handshake and cancels on manual disconnect",async()=>{
    await state.connect();await vi.advanceTimersByTimeAsync(5000);
    expect(state.error).toContain("握手超时");expect(state.connection).toBe("重连中");
    state.disconnect();await vi.advanceTimersByTimeAsync(10000);expect(Socket.all).toHaveLength(1);expect(state.connection).toBe("离线");
  });
});
