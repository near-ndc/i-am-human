#!/usr/bin/env bash
pushd ..
make build
popd
mkdir -p ./out/base/
cp ../res/* ./out/base/
#for p in /work/near/core-contracts/*/res/*.wasm ; do
#  cp $p ./out/base/
# done

cd out
for p in ./base/*.wasm ; do
  w=$(basename -- $p)
  ../minify.sh $p
  cp $p stripped-$w
  wasm-strip stripped-$w
  echo $w `stat -f %z stripped-$w` " -> " `stat -f %z minified-$w`
done