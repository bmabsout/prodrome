#!/bin/sh
# Put a store's objects beside the viewer's app, making a site.
#
#   viewer/assemble.sh APP STORE SITE
#
# APP is `nix build .#prodrome-viewer`'s output, STORE a Prodrome store (the
# `roadmap/` directory of the roadmap-data branch), SITE the directory to
# write. The objects are copied as they are on disk — each file named by the
# hash of its bytes, which is what lets the service worker keep them forever —
# and `index.json` lists their names. The Pages workflow runs exactly this.
set -eu

app=$1 store=$2 site=$3
[ -d "$store/objects" ] || { echo "$store has no objects/ directory" >&2; exit 1; }
rm -rf "$site"
mkdir -p "$site/objects"
cp -r "$app"/. "$site"/
chmod -R u+w "$site"
cp "$store"/objects/*.py "$site/objects/"
( cd "$site/objects" && ls -- *.py | sed 's/\.py$//' | sort ) \
  | awk 'BEGIN { printf "[" } { printf "%s\"%s\"", (NR > 1 ? "," : ""), $0 } END { print "]" }' \
  > "$site/index.json"
touch "$site/.nojekyll"
echo "site: $(ls "$site/objects" | wc -l) objects"
