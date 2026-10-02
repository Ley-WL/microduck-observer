"""Explicit SRAM-only controls, executed by the telemetry serial owner."""
import math
import time
from servos import ALL_IDS
from pose_calibration import read_register, write_register

ARRIVAL_TOLERANCE_TICKS = 5 * 4096 / 360  # Exactly 5 degrees; do not round up.
CONTROL_PERIOD = .02
DIRECT_SPEED_RAW = 500


def clear_legacy_profile(bus, sid, feedback):
    """Remove the old acc=5/speed=300 profile, keeping current/PID untouched."""
    if feedback.get('accelerationRaw') != 0:
        sync_write(bus, 41, {sid: b'\x00'})
    if feedback.get('speedLimitRaw') != DIRECT_SPEED_RAW:
        sync_write(bus, 46, {sid: DIRECT_SPEED_RAW.to_bytes(2, 'little')})

# Official microduck_rl robot_groundcontact.xml joint ranges, in degrees.
# Also matched against the observer's bundled model. Mouth range is user-confirmed
# hardware configuration (2026-10-01), not an official model range or stand joint.
OFFICIAL_LIMITS_DEG = {
    10: (-30, 25), 11: (-22, 22), 12: (-90, 90), 13: (-90, 90), 14: (-90, 90),
    20: (-25, 30), 21: (-22, 22), 22: (-90, 90), 23: (-90, 90), 24: (-90, 90),
    30: (-90, 60), 31: (-90, 90), 32: (-170, 170), 33: (-25, 25), 34: (0, 30),
}

# Official STAND, also used by tools/supported-stand-trial.py. Mouth has no
# official standing angle and is left unchanged by stand (enable covers all 15).
STAND = dict(zip(ALL_IDS[:-1], [0, .0872664626, .457924, .004940, -.452984,
                              0, -.0872664626, -.457924, -.004940, .452984, 0, 0, 0, 0]))


def goal_bytes(value):
    if not isinstance(value, int) or abs(value) > 32767:
        raise ValueError('目标超出编码范围')
    return (abs(value) | (0x8000 if value < 0 else 0)).to_bytes(2, 'little')


def sync_write(bus, address, values):
    size = len(next(iter(values.values())))
    params = bytes([address, size]) + b''.join(bytes([sid]) + value for sid, value in values.items())
    body = bytes([254, len(params)+2, 0x83]) + params
    packet = b'\xff\xff' + body + bytes([(~sum(body)) & 255])
    bus.serial.write(packet)
    if hasattr(bus, 'record_write'): bus.record_write(packet)


def torque_off(bus):
    # This is the FINAL register write. Never restore targets after unloading:
    # later SRAM writes have been observed to re-enable this firmware.
    sync_write(bus, 40, {sid: b'\x00' for sid in ALL_IDS})
    feedback = bus.read_feedback_many(ALL_IDS)
    missing = [sid for sid in ALL_IDS if sid not in feedback or feedback[sid]['torque'] != 0]
    return dict(state='failed' if missing else 'disabled',
                message=('已发送全部失能；未确认 ID ' + ', '.join(map(str, missing))) if missing else '15 个舵机已失能（卸力）',
                unconfirmedIds=missing)


def validate_feedback(sid, f, goal=None):
    if f is None:
        raise ValueError(f'#{sid} 反馈丢失')
    reasons = []
    if f['fault']: reasons.append(f'舵机故障码 {f["fault"]}')
    if not 4.0 <= f['voltage'] <= 8.4:
        reasons.append(f'电压 {f["voltage"]:.1f}V 超出舵机工作范围 4.0–8.4V')
    # Temperature remains telemetry. Do not impose an unverified 50 C cutoff;
    # reported servo faults are still rejected above.
    if reasons:
        raise ValueError(f'#{sid} ' + '；'.join(reasons))
    if goal is not None:
        details = (f'指令{goal} / 位置{f["position"]} / 目标回读{f.get("target", "未知")}，'
                   f'误差{goal-f["position"]}步，torque={f["torque"]}，fault={f["fault"]}，'
                   f'电流原始值{f.get("currentRaw", "未知")}，负载原始值{f["load"]}，电压{f["voltage"]:.1f}V，'
                   f'输出限制{f.get("torqueLimitRaw", "未知")}，目标电流原始值{f.get("goalCurrentRaw", "未知")}，'
                   f'加速度原始值{f.get("accelerationRaw", "未知")}，速度原始值{f.get("speedLimitRaw", "未知")}')
        if f['torque'] != 1: raise ValueError(f'#{sid} 未使能；{details}')
        # Tracking error and PWM/load are telemetry, not invented trip limits.
        # Arrival is evaluated against the final target after the trajectory.


def target_for(sid, start, calibration, low, high, angle=None, nearest=False):
    joints = calibration['joints']
    ref = joints['references'].get(str(sid))
    direction = joints['directions'].get(str(sid), -1)
    if isinstance(ref, bool) or not isinstance(ref, (int, float)) or not math.isfinite(ref) or direction not in (-1, 1):
        raise ValueError(f'#{sid} 请先完成位置标定')
    standing = angle is None
    angle = STAND[sid] if standing else angle
    minimum, maximum = OFFICIAL_LIMITS_DEG[sid]
    if not math.isfinite(angle) or not math.radians(minimum) <= angle <= math.radians(maximum):
        raise ValueError(f'#{sid} 目标超出关节角度范围 {minimum}°–{maximum}°')
    target = round(ref + angle * 4096 / (2*math.pi) * direction)
    if standing or nearest:
        target += round((start-target)/4096)*4096
    if abs(target) > 32767 or (high > low and not low <= target <= high):
        raise ValueError(f'#{sid} 站姿目标超出编码范围或舵机硬件限位')
    return target


def execute_control(bus, command, cancelled, publish, status, clock=time.monotonic, sleep=time.sleep):
    action = command['action']
    if action == 'profile':
        return execute_profile(bus, command, cancelled, publish)
    if action == 'angle':
        return execute_angle(bus, command, cancelled, publish, status)
    if action not in ('enable', 'stand'):
        raise ValueError('未知舵机操作')
    ids = tuple(STAND) if action == 'stand' else ALL_IDS
    budget = 3 if action == 'stand' else 5
    started = clock(); deadline = started + budget
    attempted = False
    last_feedback = {}; targets = {}; caps = {}; last_observed = None
    command_times = []

    def stand_target(sid, start, low, high):
        custom=command.get('standTargets')
        return target_for(sid,start,command['calibration'],low,high,
                          custom[sid] if custom else None,nearest=True)

    def timing():
        gaps = [b-a for a, b in zip(command_times, command_times[1:])]
        return dict(commandTargetHz=1/CONTROL_PERIOD, commandCount=len(command_times),
                    commandHz=round(len(gaps)/sum(gaps), 2) if gaps and sum(gaps) > 0 else None,
                    commandMaxGapMs=round(max(gaps)*1000, 2) if gaps else None)

    def guard():
        if cancelled(): raise ValueError('已取消动作，执行全部失能')
        if clock() >= deadline:
            details = []
            for sid in ids:
                f = last_feedback.get(sid)
                if f is None or sid not in targets: continue
                error = targets[sid]-f['position']
                if abs(error) > ARRIVAL_TOLERANCE_TICKS:
                    details.append((abs(error), f'#{sid} 目标{targets[sid]} / 反馈{f["position"]}，差{error}步（{error*360/4096:.2f}°），负载{f["load"]}，电压{f["voltage"]:.1f}V，输出上限{caps.get(sid, "未读取")}'))
            details.sort(key=lambda row: row[0], reverse=True)
            diagnostic = '；'.join(row[1] for row in details)
            if not diagnostic: diagnostic = '尚未满足全部关节进入±5°并稳定100ms的条件'
            age = f'（{(clock()-last_observed)*1000:.0f}ms前）' if last_observed is not None else ''
            raise ValueError(f'{budget} 秒内未确认到位，停止并失能；最后反馈{age}：{diagnostic}')

    def observe(goals=None, final=False):
        nonlocal last_feedback, last_observed
        guard()
        feedback = bus.read_feedback_many(ALL_IDS)
        last_feedback = feedback; last_observed = clock()
        publish([dict(id=sid, online=sid in feedback, **feedback.get(sid, {})) for sid in ALL_IDS])
        for sid in ids:
            validate_feedback(sid, feedback.get(sid), goals[sid] if goals is not None and not final else None)
            if final and feedback[sid]['torque'] != 1:
                raise ValueError(f'#{sid} 到位检查时未使能')
        return feedback

    try:
        status(dict(state='preflight', action=action, message='检查实时反馈、目标及硬件限位', progress=0))
        initial = observe(); targets = {}; caps = {}; limits = {}
        for sid in ids:
            guard()
            config = read_register(bus, sid, 0, 40)
            if list(config[:2]) != [3, 46] or config[33] != 4:
                raise ValueError(f'#{sid} 固件或运行模式未经验证')
            low = int.from_bytes(config[9:11], 'little'); high = int.from_bytes(config[11:13], 'little')
            limits[sid] = (low, high)
            caps[sid] = int.from_bytes(config[16:18], 'little')
            start = initial[sid]['position']
            goal_bytes(start)
            if not 0 < caps[sid] <= 1000 or (high > low and not low <= start <= high):
                raise ValueError(f'#{sid} 输出限制或当前位置超出硬件范围')
            targets[sid] = stand_target(sid,start,low,high) if action == 'stand' else start
        fresh = observe()
        guard()
        # Read-only preflight failures do not change an existing holding pose.
        starts = {sid: fresh[sid]['position'] for sid in ids}
        # Use the latest measured start instead of rejecting an arbitrary
        # eight-tick drift during register reads. Recheck true hardware bounds.
        for sid, start in starts.items():
            goal_bytes(start)
            low, high = limits[sid]
            if high > low and not low <= start <= high:
                raise ValueError(f'#{sid} 当前位置超出舵机硬件限位')
            targets[sid] = stand_target(sid,start,low,high) if action == 'stand' else start
        if action == 'stand' and deadline-clock() < 2.5:
            raise ValueError('总线预检查过慢，未执行运动')
        speed = DIRECT_SPEED_RAW
        # Raw HD1910 speed is not ticks/s. Do not infer a travel rejection
        # threshold from it or impose an arbitrary encoder-distance gate.
        attempted = True
        # First write aligns the target itself: even firmware that implicitly
        # enables on an SRAM write cannot jump toward an old stored goal.
        sync_write(bus, 42, {sid: goal_bytes(starts[sid]) for sid in ids})
        # HD1910 address 44 is target current, NOT legacy SCS move time.
        # Do not overwrite it while configuring acceleration/position/speed.
        # HD1910 register table: acceleration 0 selects maximum acceleration.
        # The host already supplies the smooth trajectory; do not add acc=5.
        sync_write(bus, 41, {sid: b'\x00' for sid in ids})
        sync_write(bus, 46, {sid: speed.to_bytes(2, 'little') for sid in ids})
        for sid in ids:
            guard()
            write_register(bus, sid, 48, caps[sid].to_bytes(2, 'little'))
            if read_register(bus, sid, 48, 2) != caps[sid].to_bytes(2, 'little'):
                raise ValueError(f'#{sid} 最大输出设置未确认')
        guard()
        sync_write(bus, 40, {sid: b'\x01' for sid in ids})
        observe(starts)
        if action == 'enable':
            return dict(state='enabled', action=action, message='15 个舵机已使能，保持当前位置', progress=1)
        began = clock()
        duration = 2.2
        if deadline-began < duration+.15: raise ValueError('总线预处理过慢，无法在 3 秒内完成')
        settled = None
        next_tick = began
        while True:
            tick = clock(); guard()
            ratio = min(1., (tick-began)/duration)
            blend = ratio*ratio*(3-2*ratio)
            goals = {sid: round(starts[sid]+(targets[sid]-starts[sid])*blend) for sid in ids}
            sync_write(bus, 42, {sid: goal_bytes(goal) for sid, goal in goals.items()})
            command_times.append(clock())
            # Once the commanded trajectory ends, use the requested arrival
            # tolerance; retain torque/fault checks and the overall deadline.
            feedback = observe(goals, final=ratio == 1)
            status(dict(state='moving', action=action, message='正在过渡到官方站姿', progress=ratio,
                        **timing(),
                        remainingSeconds=round(max(0, deadline-clock()), 1)))
            if ratio == 1 and all(abs(feedback[sid]['position']-targets[sid]) <= ARRIVAL_TOLERANCE_TICKS for sid in ids):
                if settled is None: settled = clock()
                if clock()-settled >= .1:
                    guard()
                    return dict(state='holding', action=action, progress=1,
                                **timing(),
                                message='官方站姿已到位并保持使能（14 关节；嘴部不变）', elapsedSeconds=round(clock()-started, 2))
            else: settled = None
            # Schedule against absolute deadlines; don't accumulate read time
            # or send bursts to catch up after a stalled serial transaction.
            next_tick = max(next_tick + CONTROL_PERIOD, clock())
            sleep(max(0, next_tick-clock()))
    except Exception as exc:
        stats = timing()
        if stats['commandHz'] is not None:
            exc = ValueError(f'{exc}；本次发令 {stats["commandHz"]:.2f}Hz（目标50Hz），最大间隔 {stats["commandMaxGapMs"]:.2f}ms')
        if attempted:
            try:
                off = torque_off(bus)
                raise ValueError(f'{exc}；{off["message"]}') from exc
            except ValueError: raise
            except Exception as cleanup:
                raise ValueError(f'{exc}；失能结果未知：{cleanup}') from exc
        raise ValueError(f'{exc}；预检查未通过，未发送运动指令，原使能状态不变') from exc


def validate_angle_request(sid, degrees):
    if type(sid) is not int or sid not in OFFICIAL_LIMITS_DEG:
        raise ValueError('该舵机没有已核实的官方角度范围')
    low, high = OFFICIAL_LIMITS_DEG[sid]
    if isinstance(degrees, bool) or not isinstance(degrees, (int, float)) or not math.isfinite(degrees) or not low <= degrees <= high:
        raise ValueError(f'#{sid} 角度须在配置范围 {low}°–{high}°')


def execute_angle(bus, command, cancelled, publish, status):
    """Direct target with the legacy profile cleared; no trajectory or lag gate."""
    sid, degrees = command['id'], command['angleDeg']
    validate_angle_request(sid, degrees)
    attempted = False
    def guard():
        if cancelled(): raise ValueError('角度操作已取消')
    try:
        guard()
        status(dict(state='preflight', action='angle', message=f'#{sid} 检查目标角度'))
        f = bus.read_feedback_many((sid,)).get(sid)
        validate_feedback(sid, f)
        config = read_register(bus, sid, 0, 40)
        if list(config[:2]) != [3, 46] or config[33] != 4:
            raise ValueError(f'#{sid} 固件或运行模式未经验证')
        low, high = int.from_bytes(config[9:11], 'little'), int.from_bytes(config[11:13], 'little')
        goal = target_for(sid, f['position'], command['calibration'], low, high, math.radians(degrees))
        guard()
        attempted = True
        # Target first, so clearing an old profile cannot resume an old goal.
        sync_write(bus, 42, {sid: goal_bytes(goal)})
        guard()
        clear_legacy_profile(bus, sid, f)
        guard()
        sync_write(bus, 40, {sid: b'\x01'})
        guard()
        feedback = bus.read_feedback_many(ALL_IDS)
        publish([dict(id=i, online=i in feedback, **feedback.get(i, {})) for i in ALL_IDS])
        validate_feedback(sid, feedback.get(sid), goal)
        return dict(state='commanded', action='angle', id=sid, angleDeg=degrees, target=goal,
                    message=f'#{sid} 已发送 {degrees:g}°，已使能；到位情况看实时角度')
    except Exception as exc:
        if attempted:
            off = torque_off(bus)
            raise ValueError(f'{exc}；{off["message"]}') from exc
        raise ValueError(f'{exc}；未发送运动指令') from exc


def execute_profile(bus, command, cancelled, publish):
    sid = command['id']
    validate_angle_request(sid, 0)
    if cancelled(): raise ValueError('参数修正已取消')
    feedback = bus.read_feedback_many((sid,))
    f = feedback.get(sid)
    validate_feedback(sid, f)
    config = read_register(bus, sid, 0, 40)
    if list(config[:2]) != [3, 46] or config[33] != 4:
        raise ValueError(f'#{sid} 固件或运行模式未经验证')
    if cancelled(): raise ValueError('参数修正已取消')
    clear_legacy_profile(bus, sid, f)
    if cancelled(): raise ValueError('参数修正已取消')
    feedback = bus.read_feedback_many(ALL_IDS)
    publish([dict(id=i, online=i in feedback, **feedback.get(i, {})) for i in ALL_IDS])
    after = feedback.get(sid)
    validate_feedback(sid, after)
    if after.get('accelerationRaw') != 0 or after.get('speedLimitRaw') != DIRECT_SPEED_RAW:
        raise ValueError(f'#{sid} 参数回读不一致')
    return dict(state='configured', action='profile', id=sid, accelerationRaw=0,
                speedLimitRaw=DIRECT_SPEED_RAW, message=f'#{sid} 加速度0、速度500已回读确认，未写目标角度或使能')
