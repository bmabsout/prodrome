#!/bin/sh
# Build the viewer's static app — no data in it — into OUT.
#
#   viewer/build.sh OUT PRODROME_WEB TYPST_WEB PACKAGES FONT_DIR...
#
# PRODROME_WEB and TYPST_WEB are the `web/` glue directories of
# `nix build .#prodrome-wasm` and `.#prodrome-typst-wasm`; PACKAGES is a
# Typst package path, `local/<name>/<version>/…`, holding `prodrome-typst`
# and every package it imports, each of whose Typst files is bundled; each
# FONT_DIR is searched for the fonts main.ts lists. Needs `esbuild` and `tsc` on PATH and
# nothing from npm: the TypeScript is typechecked against the glues' own
# `.d.ts` and bundled by esbuild. `nix build .#prodrome-viewer` runs this.
set -eu

out=$1 prodrome=$2 typst=$3 packages=$4
shift 4
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")

rm -rf "$here/vendor"
mkdir -p "$here/vendor"
ln -s "$prodrome" "$here/vendor/prodrome"
ln -s "$typst" "$here/vendor/typst"

# THE PACKAGES, AS TYPST-WASM READS THEM: every Typst file on the package
# path, imported as text and keyed `@local/<name>:<version>/<path>`, so the
# app holds the package path whole and no package is a special case.
find -L "$packages/local" -type f \( -name '*.typ' -o -name typst.toml \) | LC_ALL=C sort \
  > "$here/vendor/package-files"
{
  echo "// Every Typst file on the package path, by the key typst-wasm reads it at."
  n=0
  while read -r file; do
    echo "import f$n from \"$file\";"
    n=$((n + 1))
  done < "$here/vendor/package-files"
  echo "export const PACKAGES: Record<string, string> = {"
  n=0
  while read -r file; do
    rel=${file#"$packages/local/"} name=${rel%%/*}
    rest=${rel#*/}
    echo "  \"@local/$name:${rest%%/*}/${rest#*/}\": f$n,"
    n=$((n + 1))
  done < "$here/vendor/package-files"
  echo "};"
} > "$here/vendor/packages.ts"

tsc -p "$here/tsconfig.json"
tsc -p "$here/tsconfig.sw.json"

mkdir -p "$out/wasm" "$out/fonts"
cp "$prodrome/prodrome_bg.wasm" "$typst/typst_bg.wasm" "$out/wasm/"
fonts=$(sed -n 's/^  "\(.*\.otf\)",$/\1/p' "$here/src/main.ts")
for font in $fonts; do
  found=
  for dir in "$@"; do
    candidate=$(find -L "$dir" -name "$font" -print -quit)
    if [ -n "$candidate" ]; then found=$candidate; break; fi
  done
  [ -n "$found" ] || { echo "font $font not found" >&2; exit 1; }
  cp "$found" "$out/fonts/$font"
done
cp "$here"/fonts/*.md "$here"/fonts/*.txt "$out/fonts/"
cp "$here/index.html" "$here/style.css" "$out/"

# THE BUILD'S NAME: a hash of everything the app is made of, so a change to any
# of it is a new service-worker cache and new URLs for the shell.
build=$( (cat "$here"/src/*.ts "$here/vendor/packages.ts"; xargs cat < "$here/vendor/package-files"; \
          cat "$out"/index.html "$out"/style.css "$out"/wasm/* "$out"/fonts/*) \
        | sha256sum | cut -c1-16)

esbuild "$here/src/main.ts" --bundle --format=esm --target=es2022 --minify \
  --loader:.typ=text --loader:.toml=text \
  --alias:prodrome-wasm="$here/vendor/prodrome/prodrome.js" \
  --alias:typst-wasm="$here/vendor/typst/typst.js" \
  --alias:typst-packages="$here/vendor/packages.ts" \
  --define:__BUILD__="\"$build\"" \
  --outfile="$out/app.js" --log-level=warning

shell=$(cd "$out" && {
  printf '"./","index.html","app.js?v=%s","style.css?v=%s"' "$build" "$build"
  printf ',"wasm/prodrome_bg.wasm?v=%s","wasm/typst_bg.wasm?v=%s"' "$build" "$build"
  for font in $fonts; do printf ',"fonts/%s"' "$font"; done
})
esbuild "$here/src/sw.ts" --bundle --format=iife --target=es2022 --minify \
  --define:__BUILD__="\"$build\"" --define:__SHELL__="[$shell]" \
  --outfile="$out/sw.js" --log-level=warning

sed -i "s/__BUILD__/$build/g" "$out/index.html"
echo "$build" > "$out/BUILD"
rm -rf "$here/vendor"
echo "viewer $build: $(du -sh "$out" | cut -f1)"
