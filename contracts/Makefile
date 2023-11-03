##
# I Am Human

res:
	mkdir -p res

add-deps:
	rustup target add wasm32-unknown-unknown

build: res
	@RUSTFLAGS='-C link-arg=-s' cargo build --workspace --exclude test-util --target wasm32-unknown-unknown --release
	@cp target/wasm32-unknown-unknown/release/*.wasm res/

test: build
	@cargo test

# Define the target directory
TARGET_NEAR_DIR := target/near

# Custom rule for extracting and creating directories
create-near-dirs:
	@mkdir -p $(TARGET_NEAR_DIR)
	@for wasm_file in $$(ls target/wasm32-unknown-unknown/release/*.wasm); do \
		wasm_name=$$(basename "$$wasm_file" .wasm); \
		mkdir -p $(TARGET_NEAR_DIR)/$$wasm_name; \
	done
