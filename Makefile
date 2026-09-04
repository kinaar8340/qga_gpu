.PHONY: check test demo demo-tiny demo-headless headless ring ring-windowed bench bench-windowed bench-smoke bench-record bench-gradient bench-gradient-windowed bench-gradient-record

check:
	cargo check --workspace
	cargo test -p qga-gpu

test: check

# Public demo: 64×64 speakers + glass fabric + 65 536-particle ocean.
# Until Esc. Prints UploadStats on exit (static_uploads, ring_copies, …).
demo:
	cargo run -p qga-gpu-bench --release -- --scene gradient --preset 4090 --grid 64 --fluid --dirty-rings --dirty-particles

# Same scene, 30 headless frames, no capture. Stranger counter check.
demo-headless:
	cargo run -p qga-gpu-bench --release -- --headless --scene gradient --preset 4090 --grid 64 --fluid --dirty-rings --dirty-particles --frames 30 --no-capture

# 4k sculpture smoke. Not the public demo.
demo-tiny:
	cargo run -p qga-gpu-demo --release

headless:
	cargo run -p qga-gpu-demo --release -- --headless --frames 8

# Dirty particles, 300 frames. Headless captures first+last only so map_async
# stays in flight (capture Wait would hide ring pressure).
ring:
	cargo run -p qga-gpu-demo --release -- --headless --frames 300 --dirty-particles

# FIFO/mailbox presents; no capture. Exits after 300 frames.
ring-windowed:
	cargo run -p qga-gpu-demo --release -- --dirty-particles --frames 300

# 4090 QGA sculpture bench. Hopf fibers + flux motes + instanced orbs.
# Capture first+last only (headless). Does not prove inner_cone mosaic.
bench:
	cargo run -p qga-gpu-bench --release -- --headless --preset 4090 --dirty-particles --dirty-fibers --frames 600

bench-windowed:
	cargo run -p qga-gpu-bench --release -- --preset 4090 --dirty-particles --dirty-fibers --frames 600

bench-smoke:
	cargo run -p qga-gpu-bench --release -- --headless --preset smoke --no-capture

# Encode the 4090 scene to mp4 (every-frame capture Wait). Not a ring proof.
bench-record:
	cargo run -p qga-gpu-bench --release -- --preset 4090 --dirty-particles --dirty-fibers --frames 600 --record benchmarks/results/qga-gpu-bench-4090.mp4

# ngsm lattice. Instance orbs + live ring fibers. First+last capture.
bench-gradient:
	cargo run -p qga-gpu-bench --release -- --headless --scene gradient --preset 4090 --dirty-rings --frames 600

bench-gradient-windowed:
	cargo run -p qga-gpu-bench --release -- --scene gradient --preset 4090 --dirty-rings --fluid --dirty-particles --frames 600

# Same visual as bench-gradient-windowed, encoded to mp4 (every-frame capture Wait).
# --record PATH.mp4 is the flag. Grid 64, 1440p, edge-on center of the sheet.
bench-gradient-record:
	cargo run -p qga-gpu-bench --release -- --scene gradient --preset 4090 --grid 64 --width 2560 --height 1440 --dirty-rings --fluid --dirty-particles --frames 600 --record benchmarks/results/qga-gpu-bench-gradient-64.mp4
