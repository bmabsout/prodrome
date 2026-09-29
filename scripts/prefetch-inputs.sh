#!/usr/bin/env bash
# Put every github: input of a flake.lock in the Nix store by git fetch.
# A cloud session's proxy refuses GitHub's tarball downloads, which is how
# Nix fetches github: inputs, but allows git; with each input in the store
# at its locked hash, nix develop and nix flake check never ask GitHub.
#
#   scripts/prefetch-inputs.sh [flake.lock]
set -euo pipefail

lock=${1:-flake.lock}
python3 - "$lock" <<'PY' |
import json, sys
for node in json.load(open(sys.argv[1]))["nodes"].values():
    locked = node.get("locked", {})
    if locked.get("type") == "github":
        print(locked["owner"], locked["repo"], locked["rev"], locked["narHash"], sep="\t")
PY
  while IFS=$'\t' read -r owner repo rev nar; do
    dir=$(mktemp -d)
    git -C "$dir" init -q
    git -C "$dir" fetch -q --depth 1 "https://github.com/$owner/$repo" "$rev"
    git -C "$dir" checkout -q FETCH_HEAD
    rm -rf "$dir/.git"
    got=$(nix hash path "$dir")
    if [ "$got" = "$nar" ]; then
      nix store add --name source "$dir" >/dev/null
      echo "$owner/$repo ok"
    else
      echo "$owner/$repo: the checkout hashes to $got, the lock says $nar" >&2
    fi
    rm -rf "$dir"
  done
