##
# I Am Human

res:
	mkdir -p res

add-deps:
	rustup target add wasm32-unknown-unknown

build: res
	@RUSTFLAGS='-C link-arg=-s' cargo build --all --target wasm32-unknown-unknown --release
	@cp target/wasm32-unknown-unknown/release/*.wasm res/

test: build
	@cargo test
