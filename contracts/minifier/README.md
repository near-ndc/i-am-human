# Minifier

Minifier is a post-processing tool that allows to reduce the size of a contract by minifying it. For more details see the [documentation](https://docs.near.org/sdk/rust/building/post-processing).

## Usage

To use the provided scirpt the following tools must be installed: 

- `wasm-snip`
- `wasm-gc`
- `binaryen`
- `wabt`

To minify all the contracts run: `./minify_contracts.sh`.

The script will build all the contracts, then copy them to `out/base` directory. 
The stripped and minified files will be placed directly in `out` directory. 
