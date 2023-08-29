res:
	mkdir -p res

build: res
	@RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
	@mkdir -p ../res
	@cp ../target/wasm32-unknown-unknown/release/*.wasm ../res/

build-debug: res
	@RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown
	@cp ../target/wasm32-unknown-unknown/debug/*.wasm ../res/

build-abi: res
	@cargo near abi
	@cp ../target/near/*/*_abi.json ../res


build-all: res
	@RUSTFLAGS='-C link-arg=-s' cargo build --all --target wasm32-unknown-unknown --release
	@mkdir -p ../res
	@cp ../target/wasm32-unknown-unknown/release/*.wasm ../res/
	@cargo near abi
	@cp ../target/near/*/*_abi.json ../res

lint:
	cargo clippy  -- --no-deps

lint-fix:
	cargo clippy --fix  -- --no-deps


test: build
# to test specific test run: cargo test <test name>
	@cargo test

test-unit-debug:
	@RUST_BACKTRACE=1 cargo test --lib  -- --show-output

test-unit:
	@cargo test --lib
