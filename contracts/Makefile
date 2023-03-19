##
# I Am Human

add-deps:
	rustup target add wasm32-unknown-unknown

build:
	@RUSTFLAGS='-C link-arg=-s' cargo build --all --target wasm32-unknown-unknown --release
	@cp target/wasm32-unknown-unknown/release/*.wasm res/

cp-builds:
	@mkdir -p res
	@cp target/wasm32-unknown-unknown/release/*.wasm res/

test:
	@cargo test
