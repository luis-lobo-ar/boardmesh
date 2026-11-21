import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


spec = importlib.util.spec_from_file_location("smoke_gui", Path(__file__).with_name("smoke_gui.py"))
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class SmokeSceneTests(unittest.TestCase):
    def test_reads_both_raw_and_universe_backed_scenes(self):
        scene = {"objects": [{"id": "test-object"}], "selected_object_ids": ["test-object"]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "scene.json"
            for document in (scene, {"version": 1, "scene": scene, "canvases": []}):
                path.write_text(json.dumps(document))
                self.assertEqual(smoke.scene_object_count(path), 1)
                self.assertEqual(smoke.selected_object_ids(path), ["test-object"])


if __name__ == "__main__":
    unittest.main()
