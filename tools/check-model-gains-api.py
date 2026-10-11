"""Isolated simulated API persistence check. Never opens servo or IMU ports."""
import json, os, subprocess, sys, tempfile, time, urllib.request, urllib.error
from pathlib import Path

with tempfile.TemporaryDirectory() as directory:
    env=os.environ.copy()
    env.update(MICRODUCK_SOURCE='simulation',MICRODUCK_BIND='127.0.0.1:18879',MICRODUCK_SERVO_PORT='',MICRODUCK_TOF_SOCKET='',MICRODUCK_BLE_ENABLED='0',MICRODUCK_CALIBRATION_FILE=directory+'/cal.json',MICRODUCK_GAINS_FILE=directory+'/gains.json')
    def request(body=None):
        req=urllib.request.Request('http://127.0.0.1:18879/api/v1/model-gains', data=None if body is None else json.dumps(body).encode(), headers={'Content-Type':'application/json'})
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req,timeout=3) as response:return json.load(response)
    def start():
        p=subprocess.Popen([sys.argv[1]],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        for _ in range(60):
            try:request();return p
            except OSError:time.sleep(.05)
        p.terminate();p.wait();raise RuntimeError('simulator not ready')
    p=start()
    try:
        assert request()['p']==6 and request()['d']==20
        for invalid in [{'p':256,'d':20},{'p':6,'d':-1},{'p':6.2,'d':20}]:
            try:request(invalid);raise AssertionError('invalid accepted')
            except urllib.error.HTTPError as e:assert e.code==400
        assert request({'p':9,'d':24})['saved'] is True
    finally:p.terminate();p.wait(timeout=5)
    p=start()
    try:assert request()['p']==9 and request()['d']==24
    finally:p.terminate();p.wait(timeout=5)
    print(json.dumps({'default':[6,20],'savedAndRestarted':[9,24],'invalidRejected':3,'hardwareAccess':False}))
