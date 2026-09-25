"""Live observer with explicit pose calibration; no automatic motor movement."""
import asyncio
import os
import math
import time
import uuid
from pathlib import Path
from collections import deque
from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException, Query, Request, WebSocket, WebSocketDisconnect
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel
from typing import Literal

SOURCE = os.environ.get('MICRODUCK_SOURCE', 'simulation')
if SOURCE not in ('simulation', 'hardware'):
    raise ValueError('MICRODUCK_SOURCE must be simulation or hardware')
SERVO_PORT = os.environ.get("MICRODUCK_SERVO_PORT", "")
from servos import ServoSource, parse_ids
SERVO_IDS = parse_ids(os.environ.get("MICRODUCK_SERVO_IDS", "11,12,13,14,21,22,23,24"))
TOPICS = {"pose": 50, "imu.orientation": 50, "imu.raw": 50, "system": 1, "logs": None, "joints": 50}
ACTIVE_TOPICS = {k: v for k, v in TOPICS.items() if k != ('pose' if SOURCE == 'hardware' else 'imu.orientation')}
if not SERVO_PORT:
    ACTIVE_TOPICS.pop("joints", None)


class Simulator:
    def __init__(self, source='simulation'):
        self.source = source
        self.boot = str(uuid.uuid4())
        self.start = time.monotonic()
        self.mode = "motion"
        self.seq = dict.fromkeys(TOPICS, 0)
        self.latest = {}
        self.logs = deque(maxlen=2000)
        self.log("INFO", "server", "BNO085 实机观测服务启动" if source == 'hardware' else "独立模拟服务已启动，未连接真实硬件")

    def sample(self, topic, data, valid=True, timestamp=None):
        self.seq[topic] += 1
        item = dict(type="sample", protocolVersion=1, bootId=self.boot,
                    topic=topic, seq=self.seq[topic], sampleMonoMs=((timestamp if timestamp is not None else time.monotonic())-self.start)*1000,
                    source=self.source, valid=valid, data=data)
        self.latest[topic] = item
        return item

    def stamp(self, item):
        return {**item, "ageMs": max(0, (time.monotonic()-self.start)*1000-item["sampleMonoMs"])}

    def log(self, level, module, message):
        self.logs.append(self.sample("logs", dict(level=level, module=module, message=message,
                                                  eventTimeMs=int(time.time()*1000))))

    def tick(self):
        t = time.monotonic()-self.start
        if self.mode == "imu_pause":
            return
        roll = .22*math.sin(t*.8) if self.mode != "steady" else 0
        pitch = .15*math.sin(t*.55) if self.mode != "steady" else 0
        yaw = .4*math.sin(t*.3) if self.mode != "steady" else 0
        cr, sr = math.cos(roll/2), math.sin(roll/2)
        cp, sp = math.cos(pitch/2), math.sin(pitch/2)
        cy, sy = math.cos(yaw/2), math.sin(yaw/2)
        q = [sr*cp*cy-cr*sp*sy, cr*sp*cy+sr*cp*sy, cr*cp*sy-sr*sp*cy, cr*cp*cy+sr*sp*sy]
        self.sample("pose", dict(frame="robot", quaternion=q if self.mode != "invalid" else [0,0,0,0],
                                 calibrationId="simulation-identity"), self.mode != "invalid")
        gyro = [.176*math.cos(t*.8), .0825*math.cos(t*.55), .12*math.cos(t*.3)]
        # ZYX Euler rates converted to body-frame angular velocity.
        dr, dp, dy = gyro if self.mode != "steady" else [0,0,0]
        self.sample("imu.raw", dict(frame="simulated-sensor", gyro=[dr-dy*math.sin(pitch),
                    dp*math.cos(roll)+dy*math.sin(roll)*math.cos(pitch),
                    -dp*math.sin(roll)+dy*math.cos(roll)*math.cos(pitch)],
                    accel=[-9.81*math.sin(pitch),9.81*math.sin(roll)*math.cos(pitch),9.81*math.cos(roll)*math.cos(pitch)]))


sim = Simulator(SOURCE)
from calibrations import CalibrationStore, Conflict
calibrations = CalibrationStore(os.environ.get('MICRODUCK_CALIBRATION_FILE', str(Path.home()/'.microduck-observer/calibration.json')))
hardware = None
servos = None


@asynccontextmanager
async def lifespan(app):
    global hardware, servos
    if SOURCE == 'hardware':
        from hardware import HardwareSource
        hardware = HardwareSource(sim)
        hardware.thread.start()
    if SERVO_PORT:
        servos = ServoSource(sim, SERVO_PORT, SERVO_IDS)
        servos.thread.start()
    async def producer():
        count = 0
        period = .01 if hardware and servos else .02
        deadline = time.monotonic()
        while True:
            if hardware:
                hardware.drain()
            else:
                sim.tick()
            if servos:
                servos.drain()
            if count % round(1 / period) == 0:
                sim.sample("system", dict(uptimeSeconds=time.monotonic()-sim.start,
                    imu=hardware.health() if hardware else None, scenario=sim.mode if not hardware else None))
            if not hardware and count % 250 == 0:
                sim.log("INFO", "telemetry", "模拟数据源运行中 · " + sim.mode)
            count += 1
            deadline += period
            if deadline < time.monotonic() - .1:
                deadline = time.monotonic()
            await asyncio.sleep(max(0, deadline - time.monotonic()))
    task = asyncio.create_task(producer())
    yield
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass
    if hardware:
        hardware.stop.set()
        await asyncio.to_thread(hardware.thread.join, 5)
        hardware = None

    if servos:
        await asyncio.to_thread(servos.close)
        servos = None


app = FastAPI(title="MicroDuck Observer", lifespan=lifespan)
from pose_calibration import PoseCalibration, POSES
pose_calibration = PoseCalibration(sim, calibrations, lambda: servos)
app.add_middleware(CORSMiddleware, allow_origins=["http://localhost:5173", "http://127.0.0.1:5173"],
                   allow_methods=["GET", "POST"], allow_headers=["Content-Type"])


@app.get("/api/v1/info")
async def info():
    return dict(name="MicroDuck · BNO085" if SOURCE == 'hardware' else "MicroDuck Lab",
                protocolVersion=1, bootId=sim.boot, source=SOURCE,
                topics=ACTIVE_TOPICS, capabilities={"pose": SOURCE != 'hardware', "imu": True,
                "sensorOrientation": SOURCE == 'hardware', "joints": bool(SERVO_PORT),
                "camera": False, "tof": False, "scenarios": SOURCE != 'hardware'})


@app.get("/api/v1/health")
async def health():
    imu = hardware.health() if hardware else None
    return dict(status="degraded" if SOURCE == 'hardware' and (not imu or imu['state'] != 'streaming') else "ok",
                source=SOURCE, imu=imu, joints=sim.stamp(sim.latest["joints"]) if "joints" in sim.latest else None, logCount=len(sim.logs))


@app.get("/api/v1/snapshot")
async def snapshot():
    return {**{topic: sim.stamp(item) for topic, item in sim.latest.items()}, 'calibration':calibrations.read(sim.boot)}


@app.get('/api/v1/calibration')
async def get_calibration():
    return calibrations.read(sim.boot)


@app.post('/api/v1/calibration')
async def set_calibration(request: Request):
    origin=request.headers.get('origin')
    same_origin=str(request.base_url).rstrip('/')
    if origin is not None and origin not in (same_origin,'http://localhost:5173','http://127.0.0.1:5173'):
        raise HTTPException(403,'Origin not allowed')
    if request.headers.get('content-type','').split(';')[0]!='application/json': raise HTTPException(415,'JSON required')
    raw=await request.body()
    if len(raw)>16384: raise HTTPException(413,'Calibration too large')
    try:
        import json
        body=json.loads(raw)
        if not isinstance(body,dict): raise ValueError('Invalid request')
        async with pose_calibration.lock:
            return calibrations.update(body.get('revision'),body.get('patch'),sim.boot,body.get('migrate') is True)
    except Conflict as exc: raise HTTPException(409,str(exc))
    except (ValueError,TypeError) as exc: raise HTTPException(422,str(exc))


@app.get('/api/v1/calibration/poses')
def calibration_poses():
    return POSES


@app.post('/api/v1/calibration/{action}')
async def guided_calibration(action: str, request: Request):
    if action not in ('preview','execute'): raise HTTPException(404)
    origin=request.headers.get('origin')
    if origin is not None and origin not in (str(request.base_url).rstrip('/'),'http://localhost:5173','http://127.0.0.1:5173'):
        raise HTTPException(403,'Origin not allowed')
    if request.headers.get('content-type','').split(';')[0]!='application/json': raise HTTPException(415,'JSON required')
    raw=await request.body()
    if len(raw)>16384: raise HTTPException(413)
    if pose_calibration.lock.locked(): raise HTTPException(409,'已有标定任务正在执行')
    try:
        import json
        body=json.loads(raw)
        if not isinstance(body,dict): raise ValueError('Invalid request')
        async with pose_calibration.lock:
            if action=='preview': return await pose_calibration.preview(body)
            if body.get('confirm') is not True: raise ValueError('请确认姿势和写入预览')
            return await pose_calibration.execute(body.get('token'))
    except (ValueError,TypeError,KeyError,TimeoutError) as exc: raise HTTPException(409,str(exc))


@app.get("/api/v1/logs")
def logs(cursor: int = Query(0, ge=0), limit: int = Query(200, ge=1, le=2000)):
    items = [sim.stamp(x) for x in sim.logs if x["seq"] > cursor][:limit]
    return dict(items=items, nextCursor=items[-1]["seq"] if items else cursor,
                gap=bool(sim.logs and cursor and cursor < sim.logs[0]["seq"]-1))


class Scenario(BaseModel):
    name: Literal["motion", "steady", "imu_pause", "invalid", "log_burst"]


@app.post("/api/v1/debug/scenario")
def scenario(body: Scenario):
    if SOURCE == 'hardware':
        raise HTTPException(404, 'Simulator controls are unavailable on hardware')
    if body.name == "log_burst":
        for i in range(100):
            sim.log("WARN" if i % 5 == 0 else "INFO", "scenario", f"日志压力场景 · 事件 {i+1}")
    else:
        sim.mode = body.name
    sim.log("WARN" if body.name in ("imu_pause", "invalid") else "INFO", "scenario", "切换场景："+body.name)
    return dict(scenario=sim.mode)


@app.websocket("/api/v1/stream")
async def stream(ws: WebSocket):
    same_origin = ('https' if ws.url.scheme == 'wss' else 'http') + '://' + ws.headers.get('host', '')
    if ws.headers.get("origin") not in (None, same_origin, "http://localhost:5173", "http://127.0.0.1:5173"):
        await ws.close(code=1008)
        return
    await ws.accept()
    try:
        request = await asyncio.wait_for(ws.receive_json(), 5)
        if not isinstance(request, dict):
            await ws.close(code=1008)
            return
        topics = request.get("topics", {})
        if request.get("type") != "subscribe" or not isinstance(topics, dict):
            await ws.close(code=1008)
            return
        rates = {}
        for topic, value in topics.items():
            if topic not in ACTIVE_TOPICS:
                continue
            if topic == "logs":
                rates[topic] = None
            elif isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and value > 0:
                rates[topic] = min(value, TOPICS[topic])
        await ws.send_json(dict(type="subscribed", protocolVersion=1, bootId=sim.boot,
                                requestId=request.get("requestId"), topics=rates))
        sent = dict.fromkeys(rates, 0)
        deadlines = dict.fromkeys(rates, 0.)
        heartbeat = 0
        while True:
            now = time.monotonic()
            if now >= heartbeat:
                await asyncio.wait_for(ws.send_json(dict(type="heartbeat", bootId=sim.boot)), 2)
                heartbeat = now + 2
            for topic, rate in rates.items():
                if topic == "logs":
                    batch = [x for x in sim.logs if x["seq"] > sent[topic]][:100]
                    if batch:
                        await asyncio.wait_for(ws.send_json(dict(type="log_batch", items=[sim.stamp(x) for x in batch],
                            gap=bool(sent[topic] and batch[0]["seq"] > sent[topic]+1))), 2)
                        sent[topic] = batch[-1]["seq"]
                elif now >= deadlines[topic] and topic in sim.latest:
                    item = sim.latest[topic]
                    if item["seq"] > sent[topic]:
                        await asyncio.wait_for(ws.send_json(sim.stamp(item)), 2)
                        sent[topic] = item["seq"]
                    deadlines[topic] = max(now, deadlines[topic] + 1/rate)
            await asyncio.sleep(.01)
    except (WebSocketDisconnect, asyncio.TimeoutError, RuntimeError, ValueError):
        pass
    finally:
        try:
            await ws.close()
        except RuntimeError:
            pass


static_dir = Path(os.environ.get('MICRODUCK_STATIC_DIR', str(Path(__file__).parent.parent / 'frontend' / 'dist')))
if static_dir.is_dir():
    app.mount('/', StaticFiles(directory=static_dir, html=True), name='frontend')
