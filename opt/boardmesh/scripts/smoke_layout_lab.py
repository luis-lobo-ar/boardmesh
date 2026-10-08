#!/usr/bin/env python3

import json
import os
import shutil
import signal
import subprocess
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKSPACE_ROOT = ROOT.parents[1]
APP = WORKSPACE_ROOT / "target" / "debug" / "boardmesh-layout-lab"
DISPLAY = ":98"


def run(cmd, env=None, check=True, capture_output=True):
    return subprocess.run(
        cmd,
        env=env,
        check=check,
        text=True,
        capture_output=capture_output,
    )


def wait_for_window(env, title, timeout_seconds=15):
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        result = run(
            ["xdotool", "search", "--onlyvisible", "--name", title],
            env=env,
            check=False,
        )
        window_ids = [line.strip() for line in result.stdout.splitlines() if line.strip()]
        if window_ids:
            return window_ids[0]
        time.sleep(0.2)
    raise RuntimeError(f"timed out waiting for window {title!r}")


def main():
    print("building layout lab...", flush=True)
    run(
        [
            "cargo",
            "build",
            "--features",
            "gui",
            "--bin",
            "boardmesh-layout-lab",
        ],
        env=os.environ.copy(),
    )

    temp_dir = Path(tempfile.mkdtemp(prefix="mcp-layout-lab-smoke-"))
    report_path = temp_dir / "exercise-report.json"
    screenshot_path = temp_dir / "layout-lab.png"
    xvfb = None
    lab = None
    interactive = None

    try:
        xvfb = subprocess.Popen(
            ["Xvfb", DISPLAY, "-screen", "0", "1440x1000x24"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(1)

        env = os.environ.copy()
        env["DISPLAY"] = DISPLAY

        print("running internal exercise suite...", flush=True)
        lab = subprocess.run(
            [
                str(APP),
                "--exercise",
                "all",
                "--report",
                str(report_path),
            ],
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        if lab.returncode != 0:
            raise RuntimeError(
                f"layout lab exercise exited with {lab.returncode}\nstdout:\n{lab.stdout}\nstderr:\n{lab.stderr}"
            )
        if not report_path.exists():
            raise RuntimeError("exercise report was not written")

        report = json.loads(report_path.read_text())
        if report.get("scenario") != "all":
            raise RuntimeError(f"unexpected report scenario: {report.get('scenario')!r}")
        step_count = len(report.get("steps", []))
        if step_count < 600:
            raise RuntimeError(f"exercise matrix too small: {step_count} steps")
        final_summary = report.get("final_summary", "")
        if "toolbars[W:on T:on O:on]" not in final_summary:
            raise RuntimeError(f"unexpected final toolbar state: {final_summary}")
        if "dock[W:Top T:Left O:Right]" not in final_summary:
            raise RuntimeError(f"unexpected final dock state: {final_summary}")

        print(f"exercise suite passed with {step_count} steps", flush=True)

        print("capturing interactive window screenshot...", flush=True)
        interactive = subprocess.Popen(
            [str(APP)],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        window_id = wait_for_window(env, "Boardmesh Layout Lab")
        run(["import", "-display", DISPLAY, "-window", window_id, str(screenshot_path)], env=env)
        interactive.send_signal(signal.SIGTERM)
        interactive.wait(timeout=10)

        print(f"report: {report_path}", flush=True)
        print(f"screenshot: {screenshot_path}", flush=True)
    finally:
        if interactive is not None and interactive.poll() is None:
            interactive.kill()
        if xvfb is not None and xvfb.poll() is None:
            xvfb.kill()
        if lab is None and temp_dir.exists():
            shutil.rmtree(temp_dir, ignore_errors=True)


if __name__ == "__main__":
    main()
