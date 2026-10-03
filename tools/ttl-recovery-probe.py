import sys
import time
from pathlib import Path
import serial

base = Path(__file__).resolve().parents[1] / 'docs/实测记录/附件/主板/2026-10-03'
with serial.Serial('COM5', 1500000, timeout=0.2) as port:
    with (base / 'ttl-recovery.raw').open('ab') as log:
        for command in sys.argv[1:]:
            port.write((command + '\r').encode())
            deadline = time.monotonic() + 4
            while time.monotonic() < deadline:
                data = port.read(8192)
                if data:
                    log.write(data)
                    log.flush()
                    print(data.decode('utf-8', errors='replace'), end='', flush=True)
