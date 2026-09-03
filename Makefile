.PHONY: check test demo headless ring ring-windowed

check:
	cargo check --workspace
	cargo test -p qga-gpu

test: check

demo:
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
