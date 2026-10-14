#!/usr/bin/env python3
"""Read-only Linux evdev inventory/watch; never sends robot commands or pairs devices."""
import argparse
import glob
import json
import struct
import time


def ioctl_read(fd, number, size):
    import fcntl
    data = bytearray(size)
    fcntl.ioctl(fd, (2 << 30) | (size << 16) | (ord('E') << 8) | number, data, True)
    return data


def snapshot(fd):
    name = ioctl_read(fd, 0x06, 256).split(b'\0')[0].decode(errors='replace')
    bus, vendor, product, version = struct.unpack('=HHHH', ioctl_read(fd, 0x02, 8))
    abs_bits = int.from_bytes(ioctl_read(fd, 0x23, 8), 'little')
    key_bits = int.from_bytes(ioctl_read(fd, 0x21, 96), 'little')
    keys = int.from_bytes(ioctl_read(fd, 0x18, 96), 'little')
    axes = {}
    for code in range(64):
        if abs_bits & (1 << code):
            values = struct.unpack('=6i', ioctl_read(fd, 0x40 + code, 24))
            axes[str(code)] = dict(zip(('value', 'min', 'max', 'fuzz', 'flat', 'resolution'), values))
    return dict(name=name, bus=bus, vendor=f'{vendor:04x}', product=f'{product:04x}',
                version=version, axes=axes, buttons=[k for k in range(304, 318) if key_bits & (1 << k)],
                pressed=[k for k in range(304, 318) if keys & (1 << k)])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('device', nargs='?', help='/dev/input/eventN or stable by-id/by-path link')
    parser.add_argument('--watch', action='store_true', help='print changed state for the selected device')
    args = parser.parse_args()
    if args.watch and not args.device:
        parser.error('--watch requires a device')
    for path in [args.device] if args.device else sorted(glob.glob('/dev/input/event*')):
        try:
            with open(path, 'rb', buffering=0) as device:
                previous = None
                while True:
                    result = dict(device=path, **snapshot(device))
                    if result != previous:
                        print(json.dumps(result, ensure_ascii=False), flush=True)
                        previous = result
                    if not args.watch:
                        break
                    time.sleep(.05)
        except OSError as exc:
            print(json.dumps(dict(device=path, error=str(exc)), ensure_ascii=False), flush=True)
            if args.watch:
                raise SystemExit(1) from exc


if __name__ == '__main__':
    try:
        main()
    except KeyboardInterrupt:
        pass
