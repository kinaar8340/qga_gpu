.PHONY: check test demo headless

check:
	cargo check --workspace
	cargo test -p qga-gpu

test: check

demo:
	cargo run -p qga-gpu-demo --release

headless:
	cargo run -p qga-gpu-demo --release -- --headless --frames 8
