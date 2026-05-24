.PHONY: test wasm wasm-test

test:
	cargo test --workspace
	bash demystify-lua/test/run_tests.sh

wasm:
	wasm-pack build --target web --release demystify-wasm

wasm-test:
	wasm-pack test --node demystify-wasm
