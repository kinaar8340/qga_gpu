.PHONY: check test demo headless ring

check:
	cargo check --workspace
	cargo test -p qga-gpu

test: check

demo:
	cargo run -p qga-gpu-demo --release

headless:
	cargo run -p qga-gpu-demo --release -- --headless --frames 8

# Particles dirty every frame: ring_copies should track frames.
ring:
	cargo run -p qga-gpu-demo --release -- --headless --frames 8 --dirty-particles
