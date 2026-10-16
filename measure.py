#!/usr/bin/env python3
"""Compare cargo-check timings with dependencies warm and incremental disabled."""
import argparse
import datetime
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parent
MODES = {"derive": [], "field-bounds": ["field-bounds"], "scalar-bound": ["manual"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--deep", action="store_true", help="use depth 14 instead of 12")
    parser.add_argument("--repeat", type=int, default=2)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    if args.repeat < 1:
        parser.error("--repeat must be positive")
    depth = 14 if args.deep else 12
    output = ROOT / "results" / f"depth-{depth}"
    output.mkdir(parents=True, exist_ok=True)
    # Use a predictable target directory even if the caller sets CARGO_TARGET_DIR.
    env = {**os.environ, "CARGO_INCREMENTAL": "0", "CARGO_TARGET_DIR": str(ROOT / "target")}
    command = ["cargo", "check", "--lib", "--locked", "--timings"]
    if args.offline:
        command.append("--offline")
    # Prepare dependencies. This warm-up is not a measured sample.
    subprocess.run(command + ["--features", "manual"], cwd=ROOT, env=env, check=True)
    report = {
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
        "cargo": subprocess.check_output(["cargo", "-V"], text=True).strip(),
        "depth": depth,
        "method": "Sequential cargo check; incremental off; dependencies warm; source touched per run. Unit time excludes dependencies and Cargo overhead.",
        "runs": [],
    }
    # Alternate mode order between repetitions to reduce order bias.
    for repeat in range(args.repeat):
        modes = list(MODES.items())
        if repeat % 2:
            modes.reverse()
        for mode, features in modes:
            features = features + (["deep"] if args.deep else [])
            run_command = command + (["--features", ",".join(features)] if features else [])
            label = f"{mode}-{repeat + 1}"
            (ROOT / "src/lib.rs").touch()
            start = time.monotonic()
            with (output / f"{label}.log").open("w") as log:
                subprocess.run(run_command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            elapsed = time.monotonic() - start
            html = ROOT / "target/cargo-timings/cargo-timing.html"
            shutil.copyfile(html, output / f"{label}.html")
            # Cargo's HTML embeds per-unit timings as a JavaScript JSON literal.
            match = re.search(r"const UNIT_DATA = (.*?);\n", html.read_text(), re.S)
            if match is None:
                raise RuntimeError("Cargo timing HTML layout changed; inspect the saved report")
            units = [u for u in json.loads(match.group(1)) if u["name"] == "wincode-schema-read-repro"]
            if len(units) != 1:
                raise RuntimeError(f"Expected one freshly checked repro unit, got {len(units)}")
            row = {"mode": mode, "repeat": repeat + 1, "command": run_command,
                   "wall_seconds": elapsed, "unit_seconds": units[0]["duration"],
                   "sections": units[0].get("sections", [])}
            report["runs"].append(row)
            (output / "measurements.json").write_text(json.dumps(report, indent=2) + "\n")
            print(f"depth {depth}, {label}: {row['unit_seconds']:.2f}s crate, {elapsed:.2f}s wall", flush=True)


if __name__ == "__main__":
    main()
