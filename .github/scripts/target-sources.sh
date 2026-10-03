#!/usr/bin/env bash
# A CACHED target/ IS A TABULATION OF THE COMPILER, KEYED BY CONTENT.
#
# Cargo decides that a workspace crate is fresh by comparing mtimes: a source
# newer than the artefact built from it is a change. A fresh checkout gives
# every file the checkout's time, so a target/ restored from main's run
# would be rebuilt whole. This makes the comparison one of content names
# instead, git's own blob names:
#
#   record   after a build: write the blob name of every tracked file into
#            target/, beside the artefacts built from exactly those bytes.
#   restore  after a checkout: every file whose blob name is the one
#            recorded gets an mtime older than any artefact (2000-01-01), so
#            cargo reads it as unchanged; every other file keeps the
#            checkout's time, newer than every artefact, so cargo rebuilds
#            what reads it. A missing record leaves every file new.
#
# So an artefact is reused exactly when the bytes it was built from are the
# bytes checked out, whichever commit the cache came from: an older main, a
# newer one, or this pull request's base. It holds because the run that
# records is the run that builds: every artefact a later run reads was built,
# or found fresh, by the same cargo commands in the run that wrote the
# record, and a run that fails records nothing.
#
#   .github/scripts/target-sources.sh record|restore
set -euo pipefail

record=target/sources
case "${1:-}" in
  record)
    mkdir -p target
    git ls-files -s -z | sort -z > "$record"
    ;;
  restore)
    if [ ! -f "$record" ]; then
      echo "no record: every source is new"
      exit 0
    fi
    git ls-files -s -z | sort -z | comm -z -12 - "$record" | cut -z -f2- \
      | xargs -0 -r touch -h -m -d @946684800
    unchanged=$(git ls-files -s -z | sort -z | comm -z -12 - "$record" | tr -cd '\0' | wc -c)
    echo "$unchanged of $(git ls-files -z | tr -cd '\0' | wc -c) tracked files as recorded"
    ;;
  *)
    echo "usage: $0 record|restore" >&2
    exit 2
    ;;
esac
