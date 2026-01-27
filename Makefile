.PHONY: test

test:
	cargo test --workspace
	bash demystify-lua/test/run_tests.sh
