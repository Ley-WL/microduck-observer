import unittest
from unittest.mock import patch
from servos import ReadOnlyBus, ServoPoller, decode, parse_ids
import queue
import threading


def reply(sid, payload, error=0):
    body = bytes([sid, len(payload)+2, error]) + payload
    return b'\xff\xff' + body + bytes([(~sum(body)) & 255])


class FakeSerial:
    def __init__(self, response):
        self.response = bytearray(response)
        self.sent = []
        self.read_calls = 0
        self.timeout = .025
    def reset_input_buffer(self): pass
    def write(self, packet): self.sent.append(packet)
    @property
    def in_waiting(self): return len(self.response)
    def read(self, count):
        self.read_calls += 1
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

    def test_buffered_reply_uses_one_read_and_still_only_read_instruction(self):
        bus = ReadOnlyBus.__new__(ReadOnlyBus)
        bus.serial = FakeSerial(reply(24, bytes(31)))
        bus.read_feedback(24)
        self.assertEqual(bus.serial.read_calls, 1)
        self.assertEqual(bus.serial.sent[0][4], 2)

    def test_fragmented_reply_is_reassembled(self):
        class Fragmented(FakeSerial):
            @property
            def in_waiting(self): return min(3, len(self.response))
        bus = ReadOnlyBus.__new__(ReadOnlyBus)
        bus.serial = Fragmented(reply(24, bytes(31)))
        self.assertEqual(bus.read_feedback(24)['position'], 0)
        self.assertGreater(bus.serial.read_calls, 1)

    def test_timeout_is_not_zero_measurement(self):
        bus = ReadOnlyBus.__new__(ReadOnlyBus)
        bus.serial = FakeSerial(b'')
        with self.assertRaises(TimeoutError): bus.read_feedback(24)

    def test_invalid_ids_and_short_reply(self):
        for ids in ('24,24', '254', '1'):
            with self.assertRaises(ValueError): parse_ids(ids)
        with self.assertRaises(ValueError): decode(bytes(3))

    def test_latest_queue_is_bounded_and_replaces_old_frame(self):
        events=queue.Queue(maxsize=1)
        poller=ServoPoller('fake',[24],events,threading.Event())
        poller.publish([{'id':24,'position':1}])
        poller.publish([{'id':24,'position':2}],scan_ms=5)
        data,_=events.get_nowait()
        self.assertEqual(data['servos'][0]['position'],2)
        self.assertEqual(data['scanMs'],5)
        self.assertEqual(data['targetHz'],50)
        self.assertTrue(events.empty())

    def test_poller_closes_bus_and_marks_missing_servo(self):
        stop=threading.Event();events=queue.Queue(maxsize=1)
        class Bus:
            closed=False
            def __init__(self,port):pass
            def read_feedback_many(self,ids):
                stop.set();return {11:{'position':123,'received':__import__('time').monotonic()}}
            def close(self):Bus.closed=True
        with patch('servos.ReadOnlyBus',Bus):
            ServoPoller('fake',[10,11],events,stop).run()
        data,_=events.get_nowait()
        self.assertFalse(data['servos'][0]['online'])
        self.assertTrue(data['servos'][1]['online'])
        self.assertEqual(data['servos'][1]['position'],123)
        self.assertTrue(Bus.closed)

    def test_sync_read_checks_each_id_and_only_emits_read_opcode(self):
        bus=ReadOnlyBus.__new__(ReadOnlyBus)
        good=reply(11,bytes(31))
        bad=good[:-1]+bytes([good[-1]^1])
        bus.serial=FakeSerial(reply(34,bytes(31))+bad+reply(10,bytes(31))+good)
        result=bus.read_feedback_many([10,11])
        self.assertEqual(set(result),{10,11})
        sent=bus.serial.sent[0]
        self.assertEqual(list(sent[2:-1]),[254,6,0x82,40,31,10,11])
        self.assertEqual(sum(sent[2:])&255,255)
        self.assertEqual(bus.serial.timeout,.025)

    def test_sync_read_missing_id_does_not_discard_good_reply(self):
        bus=ReadOnlyBus.__new__(ReadOnlyBus)
        bus.serial=FakeSerial(reply(11,bytes(31)))
        result=bus.read_feedback_many([10,11])
        self.assertEqual(set(result),{11})
        for ids in ([],[10,10],[254]):
            with self.assertRaises(ValueError):bus.read_feedback_many(ids)
