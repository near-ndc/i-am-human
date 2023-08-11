# Contracts

## Requirements

- Rust v1.69.
  Note: near-sdk doesn't work with Rust v1.70: https://github.com/near/nearcore/issues/9143
- Cargo
- [cargo-near](https://github.com/near/cargo-near)

## Building

To create release WASM and ABI in the `res` directory, run:

```shell
cd <contract>
make build
```
