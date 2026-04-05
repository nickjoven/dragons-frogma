.PHONY: seed k-stack test check harness clean-k-stack

K_STACK_BIN := k-stack/target/release/k-stack

# Regenerate knowledge/graph/cids.lock from seed.jsonl.
# Depends on the k-stack binary (built in its own isolated workspace).
seed: $(K_STACK_BIN)
	python3 knowledge/scripts/seed.py

# Build the k-stack CID tool. It lives in its own workspace because it
# pulls git deps (ket, canon.d) that don't belong in the frogma workspace.
k-stack: $(K_STACK_BIN)

$(K_STACK_BIN):
	cd k-stack && cargo build --release

test:
	cargo test --workspace

check:
	cargo check --workspace

harness:
	cargo run -p frogma-harness --release

clean-k-stack:
	cd k-stack && cargo clean
