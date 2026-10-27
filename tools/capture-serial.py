"""Capture boot logs; receive-only unless --interrupt-boot is explicitly selected."""
import argparse
import json
import math
from pathlib import Path
import time

BOOT_MARKER = b"SPSR = 0x3c9"


def capture(port, output: Path, seconds: float, *, switch_baud: int | None = None,
            interrupt_boot: bool = False, clock=time.monotonic) -> None:
    """Stream bytes to a new directory, preserving earlier captures."""
    output.mkdir(parents=True, exist_ok=False)
    started = clock()
    count = 0
    tail = b""
    triggered = False
    interrupt_until = 0.0
    state = "failed"

    def status(phase: str) -> None:
        (output / "status.json").write_text(json.dumps({
            "phase": phase, "port": port.port, "baud": port.baudrate,
            "bytes": count, "interruptBoot": interrupt_boot,
        }, indent=2) + "\n", encoding="utf-8")

    status("listening")
    try:
        with (output / "boot.raw").open("wb") as raw, (output / "boot.txt").open("w", encoding="utf-8") as log:
            while clock() - started < seconds:
                data = port.read(8192)
                if data:
                    raw.write(data)
                    raw.flush()
                    count += len(data)
                    text = data.decode("utf-8", errors="replace")
                    log.write(text)
                    log.flush()
                    print(text, end="", flush=True)
                    tail = (tail + data)[-8192:]
                    if not triggered and BOOT_MARKER in tail:
                        triggered = True
                        if switch_baud is not None:
                            port.baudrate = switch_baud
                            print(f"\nSwitched to {switch_baud} baud", flush=True)
                        if interrupt_boot:
                            interrupt_until = clock() + 8.0
                if interrupt_boot and clock() < interrupt_until:
                    port.write(b" ")
        state = "closed"
    except KeyboardInterrupt:
        state = "interrupted"
        raise
    finally:
        status(state)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", required=True, help="COM5, COM6 or /dev/ttyUSB0")
    parser.add_argument("--baud", type=int, default=1500000)
    parser.add_argument("--seconds", type=float, default=600)
    parser.add_argument("--output", type=Path, required=True, help="New capture directory")
    parser.add_argument("--switch-baud", type=int, help="Change baud after the BL31 marker")
    parser.add_argument("--interrupt-boot", action="store_true", help="Send spaces for 8 seconds after the BL31 marker")
    args = parser.parse_args()
    if args.baud <= 0 or (args.switch_baud is not None and args.switch_baud <= 0):
        parser.error("baud rates must be positive")
    if not math.isfinite(args.seconds) or args.seconds <= 0:
        parser.error("seconds must be finite and positive")
    if args.output.exists():
        parser.error("output directory already exists; choose a new directory")
    import serial
    # Configure modem lines before opening, rather than toggling a connected board.
    port = serial.Serial(port=None, baudrate=args.baud, timeout=.03 if args.interrupt_boot else .2,
                         xonxoff=False, rtscts=False, dsrdtr=False)
    port.dtr = False
    port.rts = False
    port.port = args.port
    with port:
        capture(port, args.output, args.seconds, switch_baud=args.switch_baud,
                interrupt_boot=args.interrupt_boot)


if __name__ == "__main__":
    main()
