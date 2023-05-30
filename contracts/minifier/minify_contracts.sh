#!/usr/bin/env bash
pushd ..
make build
popd
mkdir -p ./res-min/base/
cp ../res/* ./res-min/base/

cd res-min
for p in ./base/*.wasm ; do
  w=$(basename -- $p)
  ../minify.sh $p
  cp $p stripped-$w
  wasm-strip stripped-$w
  echo $w `stat -f %z stripped-$w` " -> " `stat -f %z minified-$w`
done
