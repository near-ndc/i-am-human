build:
	@RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
	@cp ../target/wasm32-unknown-unknown/release/*.wasm ../res/

build-quick:
	@RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown
	@cp ../target/wasm32-unknown-unknown/release/*.wasm ../res/


build-all:
	@RUSTFLAGS='-C link-arg=-s' cargo build --all --target wasm32-unknown-unknown --release
	@cp ../target/wasm32-unknown-unknown/release/*.wasm ../res/


test:
# to test specific test run: cargo test <test name>
	@cargo test

test-unit-debug:
	@RUST_BACKTRACE=1 cargo test --lib  -- --nocapture
