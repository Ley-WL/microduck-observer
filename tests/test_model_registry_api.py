"""Exercise real ONNX import and selection against an isolated simulation service.

MODEL_REGISTRY_TEST_BINARY and ORT_DYLIB_PATH enable this integration test.
No robot connection, serial port, or production state directory is used.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import unittest
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.environ.get("MODEL_REGISTRY_TEST_BINARY") and os.environ.get("ORT_DYLIB_PATH"),
                     "Set test binary and matching ONNX Runtime library")
class RegistryApiTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="microduck-registry-")
        self.work = Path(self.temp.name)
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            self.port = listener.getsockname()[1]
        self.base = f"http://127.0.0.1:{self.port}/api/v1/model-registry"
        self.env = dict(os.environ, MICRODUCK_SOURCE="simulation", MICRODUCK_BLE_ENABLED="0",
                        MICRODUCK_SERVO_PORT="", MICRODUCK_TOF_SOCKET="",
                        MICRODUCK_BIND=f"127.0.0.1:{self.port}",
                        MICRODUCK_CALIBRATION_FILE=str(self.work / "calibration.json"),
                        MICRODUCK_MODEL_REGISTRY_FILE=str(self.work / "model-registry.json"),
                        MICRODUCK_POLICY_PATH=str(ROOT / "models/xgoduck_walk.onnx"))
        self.log = (self.work / "service.log").open("w", encoding="utf-8")
        self.process = None
        self.start()

    def tearDown(self):
        self.stop()
        self.log.close()
        self.temp.cleanup()

    def start(self):
        self.process = subprocess.Popen([os.environ["MODEL_REGISTRY_TEST_BINARY"]],
                                        cwd=ROOT / "backend-rust", env=self.env,
                                        stdout=self.log, stderr=subprocess.STDOUT)
        for _ in range(100):
            if self.process.poll() is not None:
                self.fail((self.work / "service.log").read_text(encoding="utf-8"))
            try:
                self.call()
                return
            except URLError:
                time.sleep(0.1)
        self.fail("Simulation service did not start")

    def stop(self):
        if self.process is not None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)

    def call(self, action="", body=None, expected=200):
        request = Request(self.base + ("/" + action if action else ""),
                          data=json.dumps(body).encode() if body is not None else None,
                          headers={"Content-Type": "application/json"})
        try:
            response = urlopen(request, timeout=30)
        except HTTPError as error:
            response = error
        with response:
            data = json.load(response)
            self.assertEqual(response.status, expected, data)
            return data

    def test_custom_action_import_binding_restart_and_bank(self):
        original = self.call()
        payload = dict(revision=original["revision"], slot="custom-wave", label="Wave weights",
                       newAction=dict(label="招手", templateSlot="dance"), presetId="dance-default",
                       onnxBase64=base64.b64encode((ROOT / "models/dance10.onnx").read_bytes()).decode())
        # A failed graph must leave neither an action nor a binding target behind.
        self.call("import", dict(payload, onnxBase64=base64.b64encode(b"invalid graph").decode()), 409)
        self.assertEqual(self.call(), original)
        self.call("import", dict(payload, slot="custom-bad", newAction=dict(label="Bad", templateSlot="walk"), presetId="walk-default"), 409)
        self.call("import", payload)
        registry = self.call()
        self.assertEqual(registry["actions"], [dict(id="custom-wave", label="招手", templateSlot="dance")])
        selected = registry["selected"]["custom-wave"]
        candidate = next(m for m in registry["models"] if m["id"] == selected)
        self.assertTrue(candidate["available"])
        self.assertEqual({k: registry["selected"][k] for k in original["selected"]}, original["selected"])
        self.call("import", dict(payload, revision=registry["revision"]), 409)
        # Add a second candidate to the new action and exercise select/rollback.
        second = dict(payload, revision=registry["revision"], label="Wave v2")
        del second["newAction"]
        self.call("import", second)
        registry = self.call()
        candidate2 = next(m["id"] for m in registry["models"] if m["slot"] == "custom-wave" and m["id"] != selected)
        self.call("select", dict(revision=registry["revision"], slot="custom-wave", id=candidate2))
        self.call("rollback", dict(revision=self.call()["revision"], slot="custom-wave"))
        self.assertEqual(self.call()["selected"]["custom-wave"], selected)
        registry_url = self.base
        self.base = self.base.replace("model-registry", "gamepad/bindings")
        info = self.call()
        self.assertIn("custom-wave", info["actions"])
        self.assertEqual(info["customActions"][0]["label"], "招手")
        bindings = dict(info["config"]["bindings"], rb="custom-wave")
        self.call(body=dict(revision=info["config"]["revision"], bindings=bindings))
        self.stop()
        self.start()
        self.assertEqual(self.call()["config"]["bindings"]["rb"], "custom-wave")
        self.base = registry_url
        self.assertEqual(self.call()["selected"]["custom-wave"], selected)
        result = subprocess.run([os.environ["MODEL_REGISTRY_TEST_BINARY"], "--verify-model-bank"],
                                env=self.env, cwd=ROOT, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        row = next(m for m in json.loads(result.stdout.splitlines()[-1])["models"] if m["kind"] == "custom-wave")
        self.assertEqual(row["executionKind"], "dance10")
        self.assertEqual(row["sha256"], candidate["metadata"]["policySha256"])

    def test_pad_bindings_save_restart_conflicts_and_reset(self):
        self.base = self.base.replace("model-registry", "gamepad/bindings")
        initial = self.call()
        bindings = dict(initial["config"]["bindings"], x="kick_left", rb="none")
        saved = self.call(body=dict(revision=initial["config"]["revision"], bindings=bindings))
        self.assertTrue(saved["saved"])
        self.assertEqual(self.call()["config"]["bindings"], bindings)
        self.call(body=dict(revision=0, bindings=bindings), expected=409)
        revision = saved["config"]["revision"]
        self.call(body=dict(revision=revision, bindings=dict(bindings, back="none")), expected=409)
        self.call(body=dict(revision=revision, bindings=dict(bindings, x="unknown_skill")), expected=409)
        self.assertEqual(self.call()["config"], saved["config"])
        self.stop()
        self.start()
        self.assertEqual(self.call()["config"], saved["config"])
        reset = self.call(body=dict(revision=revision, bindings=initial["defaults"]))
        self.assertEqual(reset["config"]["bindings"], initial["defaults"])

    def test_defaults_and_official_manifest_import(self):
        initial = self.call()
        self.assertTrue(any(p["id"] == "walk-default" for p in initial["presets"]))
        def upload(slot, filename, **config):
            return dict(revision=self.call()["revision"], slot=slot, label="Generated config",
                        onnxBase64=base64.b64encode((ROOT / "models" / filename).read_bytes()).decode(), **config)
        self.call("import", upload("walk", "xgoduck_walk.onnx", presetId="walk-default"))
        candidate = next(m for m in self.call()["models"] if m.get("imported"))
        self.assertFalse(candidate["metadata"]["hardwareValidated"])
        self.assertNotIn("sourceCheckpoint", candidate["metadata"])
        self.assertEqual(candidate["metadata"]["walkExecution"]["policySha256"], candidate["metadata"]["policySha256"])
        manifest = {"schema_version": 2, "model_api": 1, "obs_len": 61, "action_len": 14,
                    "robot": {"model": "microduck", "servos": "xl330", "control_hz": 50},
                    "policies": [{"file": "ball_kick_left.onnx", "name": "kick_left", "kind": "episodic", "duration_s": 0.5}]}
        body = upload("kick_left", "kick_left.onnx", manifest=manifest, onnxFilename="ball_kick_left.onnx")
        self.call("import", body)
        body["revision"] = self.call()["revision"]
        body["slot"] = "kick_right"
        self.call("import", body, 409)
        body["slot"] = "kick_left"
        body["manifest"]["model_api"] = 2
        self.call("import", body, 409)
        flamingo = json.loads((ROOT / "models/flamingo_official.metadata.json").read_text(encoding="utf-8"))["sourceManifest"]
        self.call("import", upload("flamingo", "flamingo_official.onnx", manifest=flamingo, onnxFilename="policy.onnx"))
        self.assertEqual(self.call()["selected"], initial["selected"])

    def test_import_select_rollback_restart_and_failed_transactions(self):
        initial = self.call()
        self.assertEqual(initial["selected"]["flamingo"], "flamingo-right")
        self.assertTrue(next(m for m in initial["models"] if m["id"] == "walk-default")["available"])
        metadata = json.loads((ROOT / "models/xgoduck_walk.metadata.json").read_text(encoding="utf-8"))
        weights = (ROOT / "models/xgoduck_walk.onnx").read_bytes()
        upload = dict(revision=initial["revision"], slot="walk", label="API test candidate",
                      metadata=metadata, onnxBase64=base64.b64encode(weights).decode())
        self.call("import", upload)
        imported = self.call()
        self.assertEqual(imported["selected"], initial["selected"])
        candidate = next(m for m in imported["models"] if m.get("imported"))
        self.assertTrue(candidate["available"])
        self.call("select", dict(revision=initial["revision"], slot="walk", id=candidate["id"]), 409)
        self.call("select", dict(revision=imported["revision"], slot="flamingo", id=candidate["id"]), 409)
        self.assertEqual(self.call()["revision"], imported["revision"])
        self.call("select", dict(revision=imported["revision"], slot="walk", id=candidate["id"]))
        selected = self.call()
        self.assertEqual(selected["selected"]["walk"], candidate["id"])
        self.assertEqual(selected["previous"]["walk"], "walk-default")
        self.stop()
        self.start()
        self.assertEqual(self.call()["selected"], selected["selected"])
        self.call("rollback", dict(revision=selected["revision"], slot="walk"))
        rolled = self.call()
        self.assertEqual(rolled["selected"]["walk"], "walk-default")
        self.assertEqual(rolled["previous"]["walk"], candidate["id"])
        # Wrong SHA and a corrupt graph with a correct SHA must both leave state untouched.
        for correct_hash in (False, True):
            corrupt = b"not an ONNX graph"
            bad_meta = dict(metadata)
            if correct_hash:
                bad_meta["policySha256"] = hashlib.sha256(corrupt).hexdigest()
                bad_meta["walkExecution"] = dict(metadata["walkExecution"], policySha256=bad_meta["policySha256"])
            self.call("import", dict(upload, revision=rolled["revision"], metadata=bad_meta,
                                     onnxBase64=base64.b64encode(corrupt).decode()), 409)
            self.assertEqual(self.call(), rolled)
        self.assertEqual(len(list((self.work / "model-artifacts").iterdir())), 1)
        # A failure in the loader must not poison the registry or prevent the next valid selection.
        self.call("select", dict(revision=rolled["revision"], slot="walk", id=candidate["id"]))
        before_failure = self.call()
        # Force a persistence failure without changing the running in-memory selection.
        state_file = self.work / "model-registry.json"
        backup = self.work / "model-registry.backup.json"
        state_file.rename(backup)
        state_file.mkdir()
        try:
            self.call("rollback", dict(revision=before_failure["revision"], slot="walk"), 409)
            self.assertEqual(self.call(), before_failure)
        finally:
            state_file.rmdir()
            backup.rename(state_file)


if __name__ == "__main__":
    unittest.main()
