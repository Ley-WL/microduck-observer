"""Run on the board only after HAT/TTL removal and TX/RX jumper confirmation."""
import argparse
import json
import subprocess
import urllib.request


def compare(payload, received):
    return {'txBytes': len(payload), 'rxBytes': len(received),
            'exactMatch': received == payload, 'rxHex': received.hex()}


def check(port):
    import os
    import select
    import termios
    import time
    if port != '/dev/ttyS2':
        raise ValueError('Unsupported UART device')
    active = subprocess.run(['systemctl', 'is-active', 'microduck-observer'],
                            capture_output=True, text=True).stdout.strip() == 'active'
    if active:
        snapshot = json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot', timeout=5))
        state = snapshot['system']['data']['servoControl']['state']
        if state not in ['idle', 'failed', 'disabled', 'holding', 'commanded', 'enabled', 'configured']:
            raise RuntimeError('Active robot task; loopback refused: ' + state)
        subprocess.run(['systemctl', 'stop', 'microduck-observer'], check=True)
    rows = []
    try:
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        try:
            for baud, rate in [(115200, termios.B115200), (1000000, termios.B1000000)]:
                attrs = termios.tcgetattr(fd)
                attrs[0] = attrs[1] = attrs[3] = 0
                attrs[2] = termios.CLOCAL | termios.CREAD | termios.CS8
                attrs[4] = attrs[5] = rate
                attrs[6][termios.VMIN] = attrs[6][termios.VTIME] = 0
                termios.tcsetattr(fd, termios.TCSANOW, attrs)
                time.sleep(.1)
                termios.tcflush(fd, termios.TCIOFLUSH)
                for index in range(3):
                    termios.tcflush(fd, termios.TCIFLUSH)
                    payload = b'UART2-LOOPBACK-' + bytes([index]) + bytes(range(32))
                    # Full write even if the OS returns a partial write.
                    pending = memoryview(payload)
                    while pending:
                        sent = os.write(fd, pending)
                        if sent <= 0:
                            raise RuntimeError('Zero-byte serial write')
                        pending = pending[sent:]
                    received = b''
                    began = time.monotonic()
                    while time.monotonic() - began < .25:
                        if select.select([fd], [], [], .01)[0]:
                            received += os.read(fd, 8192)
                    rows.append({'baud': baud, 'round': index + 1, **compare(payload, received)})
        finally:
            os.close(fd)
    finally:
        if active:
            subprocess.run(['systemctl', 'start', 'microduck-observer'], check=True)
    return {'port': port, 'results': rows, 'allMatched': all(r['exactMatch'] for r in rows),
            'motionCommands': 0, 'serviceWasActive': active, 'serviceRestored': active}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', default='/dev/ttyS2')
    parser.add_argument('--confirmed-isolated', action='store_true', required=True,
                        help='Operator confirmed HAT/TTL unplugged and TX/RX-only jumper')
    args = parser.parse_args()
    result = check(args.port)
    print(json.dumps(result))
    raise SystemExit(0 if result['allMatched'] else 2)
