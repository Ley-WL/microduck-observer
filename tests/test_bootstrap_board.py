import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('bootstrap', Path(__file__).parents[1] / 'deploy/bootstrap_board.py')
bootstrap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bootstrap)


class BootstrapTests(unittest.TestCase):
    def test_model_defaults_and_saved_settings(self):
        defaults = Path(__file__).parents[1] / 'deploy/model-gains.default.json'
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp)
            target = bootstrap.seed_model_gains(state, defaults)
            self.assertEqual(json.loads(target.read_text()), {'p': 5, 'd': 0})
            saved = '{"p":7,"d":9}\n'
            target.write_text(saved)
            bootstrap.seed_model_gains(state, defaults)
            self.assertEqual(target.read_text(), saved)

    def test_boot_configuration_keeps_uuid_and_user_overlays(self):
        text = ('fdtfile=rockchip/rk3566-radxa-zero3.dtb\nrootdev=UUID=keep-me\n'
                'console=both\noverlay_prefix=rk35xx\noverlays=spi3 uart2-m0\nuser_overlays=custom\n')
        result = bootstrap.boot_configuration(text)
        self.assertIn('rootdev=UUID=keep-me', result)
        self.assertIn('user_overlays=custom', result)
        self.assertIn('overlay_prefix=rk3568', result)
        self.assertIn('overlays=spi3 uart2-m0 uart9-m1 i2c3-pihat aic3104-i2c3', result)
        self.assertEqual(bootstrap.boot_configuration(result), result)

    def test_wrong_board_is_rejected(self):
        with self.assertRaises(RuntimeError):
            bootstrap.boot_configuration('fdtfile=another-board.dtb\n')

    def test_existing_calibration_identity_and_broken_current_are_protected(self):
        for name in ['var/lib/ducklink/identity.json', 'var/lib/microduck-observer/calibration.json',
                     'home/radxa/microduck-observer/current']:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                bootstrap.require_fresh(root)
                file = root / name
                file.parent.mkdir(parents=True)
                if name.endswith('current'):
                    file.symlink_to(root / 'missing-release')
                else:
                    file.write_text('preserve')
                with self.assertRaises(RuntimeError):
                    bootstrap.require_fresh(root)

    def test_package_traversal_unrelated_paths_and_links_are_rejected(self):
        for name, link in [('release/../../escape', False), ('system/etc/shadow', False),
                           ('system/usr/local/lib/microduck-services-evil/file', False), ('release/link', True)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                archive = root / 'payload.tar'
                with tarfile.open(archive, 'w') as tar:
                    item = tarfile.TarInfo(name)
                    if link:
                        item.type = tarfile.SYMTYPE
                        item.linkname = '/etc/shadow'
                        tar.addfile(item)
                    else:
                        item.size = 1
                        tar.addfile(item, io.BytesIO(b'x'))
                with self.assertRaises(RuntimeError):
                    bootstrap.unpack(archive, root / 'output')

    def test_valid_bundle_layout_extracts(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            archive = root / 'payload.tar'
            with tarfile.open(archive, 'w') as tar:
                item = tarfile.TarInfo('release/bin/microduck-observer')
                item.size = 3
                tar.addfile(item, io.BytesIO(b'ELF'))
            bootstrap.unpack(archive, root / 'output')
            self.assertEqual((root / 'output/release/bin/microduck-observer').read_bytes(), b'ELF')


if __name__ == '__main__':
    unittest.main()
