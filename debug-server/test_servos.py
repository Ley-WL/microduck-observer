import unittest
from unittest.mock import patch
from servos import ReadOnlyBus, decode, parse_ids


def reply(sid, payload, error=0):
    body = bytes([sid, len(payload)+2, error]) + payload
    return b'\xff\xff' + body + bytes([(~sum(body)) & 255])


class FakeSerial:
    def __init__(self, response):
        self.response = bytearray(response)
        self.sent = []
    def reset_input_buffer(self): pass
    def write(self, packet): self.sent.append(packet)
    def read(self, count):
        chunk = self.response[:count]
        del self.response[:count]
        return bytes(chunk)


class ServoTests(unittest.TestCase):
    def test_decode_units_and_signed_raw_values(self):
        data = bytearray(31)
        data[16:18] = (2048).to_bytes(2, 'little')
        data[20:22] = (1029).to_bytes(2, 'little')
        data[22:24] = bytes([82, 35])
        data[29:31] = (32770).to_bytes(2, 'little')
        result = decode(data, 4)
        self.assertEqual(result['position'], 2048)
        self.assertEqual(result['voltage'], 8.2)
        self.assertEqual(result['currentRaw'], -2)
        self.assertEqual(result['load'], -5)
        self.assertEqual(result['fault'], 4)
        self.assertIsNone(result['angle'])

    def test_only_read_and_discard_wrong_id_corruption_and_echo(self):
        payload = bytes(31)
        valid = reply(24, payload)
        corrupt = valid[:-1] + bytes([valid[-1] ^ 1])
        echo = bytes([255,255,24,4,2,40,31,154])
        bus = ReadOnlyBus.__new__(ReadOnlyBus)
        bus.serial = FakeSerial(echo + reply(23, payload) + corrupt + valid)
        self.assertEqual(bus.read_feedback(24)['position'], 0)
        self.assertEqual(bus.serial.sent, [bytes([255,255,24,4,2,40,31,154])])

    def test_timeout_is_not_zero_measurement(self):
        bus = ReadOnlyBus.__new__(ReadOnlyBus)
        bus.serial = FakeSerial(b'')
        with self.assertRaises(TimeoutError): bus.read_feedback(24)

    def test_invalid_ids_and_short_reply(self):
        for ids in ('24,24', '254', '1'):
            with self.assertRaises(ValueError): parse_ids(ids)
        with self.assertRaises(ValueError): decode(bytes(3))
