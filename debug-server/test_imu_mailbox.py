import multiprocessing
import time
import unittest

from imu_mailbox import ImuMailbox
from ms901m import Ms901mSource


def reader(mailbox, result):
    result.put(mailbox.snapshot())


class MailboxTests(unittest.TestCase):
    def test_acquisition_updates_worker_without_web_drain(self):
        context = multiprocessing.get_context('spawn')
        mailbox = ImuMailbox(context)
        store = type('Store', (), dict(source='hardware', boot='test-boot'))()
        source = Ms901mSource(store)
        source.policy_sink = mailbox.update
        for i in range(600):
            source.put('quaternion', [0, 0, 0, 1], 10+i/200)
            source.put('raw', dict(gyro=[i, 0, 0], accel=[0, 0, 9.81]), 10+i/200)
        # The UI event buffer has overflowed, but latest policy data is current.
        self.assertGreater(source.dropped, 0)
        result = context.Queue()
        child = context.Process(target=reader, args=(mailbox, result))
        child.start()
        snapshot = result.get(timeout=10)
        child.join(10)
        self.assertEqual(child.exitcode, 0)
        self.assertEqual(snapshot['imu.raw']['data']['gyro'][0], 599)
        self.assertAlmostEqual(snapshot['imu.orientation']['received'], 12.995)
        self.assertEqual(snapshot, mailbox.snapshot())
        result.close()

    def test_missing_and_old_data_are_not_refreshed_by_reading(self):
        mailbox = ImuMailbox(multiprocessing.get_context('spawn'))
        self.assertEqual(mailbox.snapshot(), {})
        mailbox.update({'imu.raw': dict(received=1., valid=False)})
        self.assertEqual(mailbox.snapshot()['imu.raw']['received'], 1.)
        self.assertFalse(mailbox.snapshot()['imu.raw']['valid'])


if __name__ == '__main__':
    unittest.main()

