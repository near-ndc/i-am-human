build:
	@RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
	@cp ../target/wasm32-unknown-unknown/release/*.wasm ../res/

build-debug:
	@RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown
	@cp ../target/wasm32-unknown-unknown/debug/*.wasm ../res/

build-abi:
	@cargo near abi
	@cp ../target/near/*/*_abi.json ../res


build-all:
	@RUSTFLAGS='-C link-arg=-s' cargo build --all --target wasm32-unknown-unknown --release
	@cp ../target/wasm32-unknown-unknown/release/*.wasm ../res/
	@cargo near abi
	@cp ../target/near/*/*_abi.json ../res

lint:
	cargo clippy  -- --no-deps

lint-fix:
	cargo clippy --fix  -- --no-deps


test:
# to test specific test run: cargo test <test name>
	@cargo test

test-unit-debug:
	@RUST_BACKTRACE=1 cargo test --lib  -- --show-output

test-unit:
	@cargo test --lib
