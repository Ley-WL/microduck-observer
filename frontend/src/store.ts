import { computed, ref, shallowRef, onScopeDispose } from "vue";
import { defineStore } from "pinia";
import {
  euler,
  freshness,
  isSample,
  validQuaternion,
  vector,
  type Sample,
} from "./protocol";

export const useTelemetry = defineStore("telemetry", () => {
  const endpoint = ref(
    import.meta.env.DEV
      ? localStorage.getItem("microduck-endpoint") || "http://127.0.0.1:8877"
      : window.location.origin,
  );
  const connection = ref("离线"),
    error = ref(""),
    info = ref<any>(null),
    paused = ref(false);
  const now = ref(performance.now()),
    received = ref(0),
    hz = ref(0),
    boot = ref("");
  const pose = shallowRef<Sample | null>(null),
    sensorOrientation = shallowRef<Sample | null>(null),
    imu = shallowRef<Sample | null>(null);
  const orientation = computed(() => pose.value || sensorOrientation.value);
  const sensorOnly = computed(
    () =>
      info.value?.capabilities?.sensorOrientation &&
      !info.value?.capabilities?.pose,
  );
  const logs = shallowRef<Sample[]>([]),
    logGap = ref(false),
    rejected = ref(0);
  const chart = shallowRef<{ t: number; gyro: number[]; accel: number[] }[]>(
    [],
  );
  const imuReceived = ref(0);
  const imuAge = computed(() =>
    imu.value
      ? imu.value.ageMs + Math.max(0, now.value - imuReceived.value)
      : Infinity,
  );
  const imuState = computed(() =>
    !imu.value ? "缺失" : freshness(imuAge.value, imu.value.valid),
  );
  const age = computed(() =>
    orientation.value
      ? orientation.value.ageMs + Math.max(0, now.value - received.value)
      : Infinity,
  );
  const poseState = computed(() =>
    !orientation.value ? "缺失" : freshness(age.value, orientation.value.valid),
  );
  const angles = computed(() =>
    orientation.value?.valid ? euler(orientation.value.data.quaternion) : null,
  );
  const source = computed(() =>
    info.value?.source === "simulation"
      ? "模拟数据"
      : info.value?.source === "hardware"
        ? "实机数据"
        : info.value?.source === "replay"
          ? "回放数据"
          : "未连接",
  );
  let ws: WebSocket | null = null,
    retry: ReturnType<typeof setTimeout> | undefined,
    generation = 0,
    attempts = 0;
  let lastMessage = 0,
    lastHz = performance.now(),
    frameCount = 0,
    chartLast = 0;
  let sequences: Record<string, number> = {};
  let pendingLogs: Sample[] = [],
    history = chart.value;
  function reset(nextBoot: string) {
    boot.value = nextBoot;
    sequences = {};
    pose.value = null;
    sensorOrientation.value = null;
    imu.value = null;
    logs.value = [];
    pendingLogs = [];
    history = [];
    chart.value = [];
    logGap.value = false;
    hz.value = 0;
    frameCount = 0;
    chartLast = 0;
  }
  function accept(item: unknown) {
    if (!isSample(item)) {
      rejected.value++;
      return;
    }
    if (boot.value !== item.bootId) reset(item.bootId);
    if (item.seq <= (sequences[item.topic] ?? -1)) return;
    if (
      item.topic === "logs" &&
      sequences.logs !== undefined &&
      item.seq > sequences.logs + 1
    )
      logGap.value = true;
    sequences[item.topic] = item.seq;
    if (item.topic === "pose" || item.topic === "imu.orientation") {
      const valid =
        item.valid &&
        item.data.frame === (item.topic === "pose" ? "robot" : "sensor") &&
        validQuaternion(item.data.quaternion);
      if (item.topic === "pose") pose.value = { ...item, valid };
      else sensorOrientation.value = { ...item, valid };
      received.value = performance.now();
      frameCount++;
    } else if (item.topic === "imu.raw") {
      const valid =
        item.valid && vector(item.data.gyro, 3) && vector(item.data.accel, 3);
      imu.value = { ...item, valid };
      imuReceived.value = performance.now();
      if (valid && item.sampleMonoMs - chartLast >= 95) {
        chartLast = item.sampleMonoMs;
        history.push({
          t: item.sampleMonoMs / 1000,
          gyro: item.data.gyro,
          accel: item.data.accel,
        });
        if (history.length > 600) history.splice(0, history.length - 600);
      }
    } else if (item.topic === "logs") {
      if (
        typeof item.data.message !== "string" ||
        typeof item.data.module !== "string" ||
        !["DEBUG", "INFO", "WARN", "ERROR"].includes(item.data.level)
      ) {
        rejected.value++;
        return;
      }
      pendingLogs.push(item);
    }
  }
  const timer = setInterval(() => {
    now.value = performance.now();
    if (pendingLogs.length) {
      logs.value = [...logs.value, ...pendingLogs].slice(-5000);
      pendingLogs = [];
    }
    if (!paused.value) chart.value = [...history];
    if (now.value - lastHz >= 1000) {
      hz.value = (frameCount * 1000) / (now.value - lastHz);
      frameCount = 0;
      lastHz = now.value;
    }
    if (ws?.readyState === WebSocket.OPEN && now.value - lastMessage > 6000) {
      error.value = "服务心跳超时";
      ws.close();
    }
  }, 100);
  function base(): string {
    const u = new URL(endpoint.value);
    if (
      !["http:", "https:"].includes(u.protocol) ||
      u.username ||
      u.password ||
      u.search ||
      u.hash
    )
      throw new Error("请输入 HTTP(S) 服务地址，不含凭据或查询参数");
    return u.href.replace(/\/$/, "");
  }
  async function json(path: string, init?: RequestInit) {
    const response = await fetch(base() + path, {
      ...init,
      signal: AbortSignal.timeout(5000),
    });
    if (!response.ok) throw new Error(`服务返回 HTTP ${response.status}`);
    return response.json();
  }
  function disconnect() {
    generation++;
    clearTimeout(retry);
    ws?.close();
    ws = null;
    connection.value = "离线";
  }
  async function connect(manual = true) {
    if (manual) {
      disconnect();
      attempts = 0;
      reset("");
      info.value = null;
    }
    const id = generation;
    connection.value = attempts ? "重连中" : "连接中";
    error.value = "";
    function reconnect(reason: string) {
      if (id !== generation) return;
      connection.value = "重连中";
      error.value = reason;
      clearTimeout(retry);
      retry = setTimeout(
        () => connect(false),
        Math.min(10000, 700 * 2 ** attempts++) + Math.random() * 250,
      );
    }
    try {
      const metadata = await json("/api/v1/info");
      if (id !== generation) return;
      if (metadata.protocolVersion !== 1) {
        connection.value = "不兼容";
        error.value = "服务协议主版本不兼容";
        return;
      }
      info.value = metadata;
      if (boot.value !== metadata.bootId) reset(metadata.bootId);
      localStorage.setItem("microduck-endpoint", base());
      const socket = new WebSocket(
        base().replace(/^http/, "ws") + "/api/v1/stream",
      );
      ws = socket;
      socket.onopen = () => {
        if (id !== generation) {
          socket.close();
          return;
        }
        lastMessage = performance.now();
        socket.send(
          JSON.stringify({
            type: "subscribe",
            requestId: "dashboard",
            topics: {
              pose: 50,
              "imu.orientation": 50,
              "imu.raw": 50,
              system: 1,
              logs: null,
            },
          }),
        );
      };
      socket.onmessage = (event) => {
        if (id !== generation) return;
        lastMessage = performance.now();
        try {
          const message = JSON.parse(event.data);
          if (message.type === "subscribed") {
            if (message.protocolVersion !== 1) {
              disconnect();
              connection.value = "不兼容";
              return;
            }
            attempts = 0;
            connection.value = "在线";
            error.value = "";
          } else if (
            message.type === "log_batch" &&
            Array.isArray(message.items)
          ) {
            if (message.gap) logGap.value = true;
            for (const item of message.items) accept(item);
          } else if (message.type === "sample") accept(message);
        } catch {
          rejected.value++;
        }
      };
      socket.onclose = () => reconnect("连接断开，正在自动重试");
      socket.onerror = () => socket.close();
    } catch (e) {
      reconnect(e instanceof Error ? e.message : "无法连接服务");
    }
  }
  async function scenario(name: string) {
    try {
      await json("/api/v1/debug/scenario", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name }),
      });
    } catch (e) {
      error.value = String(e);
    }
  }
  function dispose() {
    disconnect();
    clearInterval(timer);
  }
  onScopeDispose(dispose);
  return {
    endpoint,
    connection,
    error,
    info,
    paused,
    pose,
    orientation,
    sensorOnly,
    imu,
    logs,
    logGap,
    rejected,
    chart,
    hz,
    age,
    poseState,
    imuAge,
    imuState,
    angles,
    source,
    connect,
    disconnect,
    scenario,
    dispose,
  };
});

// Connection closures must restart together when their implementation changes.
if (import.meta.hot) import.meta.hot.accept(() => window.location.reload());
