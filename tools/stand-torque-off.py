import os, signal, sys, time, json
sys.path.insert(0,'/home/radxa/microduck-observer/current/debug-server')
from servos import ReadOnlyBus, ALL_IDS
from pose_calibration import write_register, read_register
import serial
pid=int(sys.argv[1]); bus=None
try:
    os.kill(pid,signal.SIGSTOP);time.sleep(.1)
    bus=ReadOnlyBus.__new__(ReadOnlyBus)
    bus.serial=serial.Serial('/dev/serial/by-id/usb-1a86_USB_Single_Serial_5B79076110-if00',1000000,timeout=.025,write_timeout=.1,exclusive=False)
    for sid in ALL_IDS:write_register(bus,sid,40,b'\x00')
    time.sleep(.1)
    print(json.dumps({'torqueOffReadback':{sid:read_register(bus,sid,40,1)[0] for sid in ALL_IDS}}))
finally:
    if bus:bus.close()
    os.kill(pid,signal.SIGCONT)
