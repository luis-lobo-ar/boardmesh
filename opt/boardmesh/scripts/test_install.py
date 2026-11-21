import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


class StandaloneInstallTests(unittest.TestCase):
    def test_clean_install_needs_no_source_scene_and_preserves_existing_board(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            product = root / "opt/boardmesh"
            product.mkdir(parents=True)
            installer = product / "install.sh"
            shutil.copy2(Path(__file__).resolve().parents[1] / "install.sh", installer)
            release = root / "target/release"
            release.mkdir(parents=True)
            for name in ("boardmesh", "boardmesh-mcp"):
                binary = release / name
                binary.write_text("#!/bin/sh\nexit 0\n")
                binary.chmod(0o755)
            installed = root / "installed"
            command = ["bash", str(installer), "--skip-build", "--no-codex", "--no-stop",
                       "--no-restart-gui", "--install-root", str(installed)]
            result = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            scene = installed / "boardmesh-sessions/default.json"
            self.assertEqual(json.loads(scene.read_text())["objects"], [])
            existing = b'{"objects":[],"test":"preserve user board"}\n'
            scene.write_bytes(existing)
            subprocess.run(command, check=True, capture_output=True)
            self.assertEqual(scene.read_bytes(), existing)


if __name__ == "__main__":
    unittest.main()
