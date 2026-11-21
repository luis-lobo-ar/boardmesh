#!/usr/bin/env python3
"""Assemble only the public Boardmesh manuals into a static documentation site."""
import argparse
from pathlib import Path
import shutil


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    product = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if output == product or product in output.parents:
        parser.error("Output must be outside the product source directory")
    if output.exists() and any(output.iterdir()):
        parser.error("Output must be an empty directory")
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(product / "README.html", output / "README.html")
    shutil.copy2(product / "README.json", output / "README.json")
    shutil.copytree(product / "docs", output / "docs",
                    ignore=shutil.ignore_patterns("projects", "samples", "__pycache__"))
    (output / ".nojekyll").touch()
    (output / "index.html").write_text(
        '<!doctype html><html lang="en"><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width,initial-scale=1">'
        '<meta http-equiv="refresh" content="0;url=docs/README.html">'
        '<title>Boardmesh documentation</title></head><body>'
        '<p><a href="docs/README.html">Read the Boardmesh documentation</a>.</p>'
        '</body></html>\n'
    )
    print(f"Built Boardmesh documentation site: {output}")


if __name__ == "__main__":
    main()
