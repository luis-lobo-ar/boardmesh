#!/usr/bin/env python3
"""Compare actual legacy mesh-graph and ortho-router geometry in an isolated Rust project."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--toolchain", default="1.95.0")
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve()
    graph = source.parents[3] / "lib/mesh-graph"
    with tempfile.TemporaryDirectory(prefix="boardmesh-routing-comparison-") as directory:
        root = Path(directory)
        (root / "src").mkdir()
        (root / "src/main.rs").write_text(source.with_name("routing_comparison.rs").read_text())
        (root / "Cargo.toml").write_text(
            '[package]\nname="boardmesh-routing-comparison"\nversion="0.0.0"\nedition="2024"\npublish=false\n'
            '[workspace]\n[dependencies]\nmesh-graph={path=' + json.dumps(str(graph)) + '}\n'
            'ortho-router="=0.1.1"\nserde_json="1.0"\n'
        )
        command = ["cargo", "+" + args.toolchain, "run", "--release", "--manifest-path", str(root / "Cargo.toml")]
        if args.target_dir:
            command += ["--target-dir", str(args.target_dir.resolve())]
        result = subprocess.run(command, check=True, capture_output=True, text=True)
        cases = json.loads(result.stdout)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({"ortho_router_version": "0.1.1", "comparison": "geometry, not latency",
                                      "cases": cases}, indent=2) + "\n")
    print(f"Compared {len(cases)} fixtures. Report: {args.output}")


if __name__ == "__main__":
    main()
