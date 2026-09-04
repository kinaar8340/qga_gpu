#!/usr/bin/env bash
# Headless smoke then 4090. No per-frame capture (Wait hides ring pressure).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
SHA="$(git rev-parse --short HEAD)"
mkdir -p benchmarks/results

cargo run -p qga-gpu-bench --release -- \
  --headless --preset smoke --no-capture \
  --json "benchmarks/results/${SHA}-smoke.json"

cargo run -p qga-gpu-bench --release -- \
  --headless --preset 4090 --dirty-particles --dirty-fibers --no-capture --frames 600 \
  --json "benchmarks/results/${SHA}-4090.json"
