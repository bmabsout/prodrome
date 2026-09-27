#!/usr/bin/env bash
# Commit what `prodrome github` appended to a checkout of roadmap-data, and
# push it. Used by mirror.yml and price.yml; run from that checkout.
#
#   roadmap-push.sh MESSAGE
#
# NOTHING TO COMMIT IS SUCCESS: a replayed delivery appends nothing. A push
# that loses a race with another workflow run merges and tries again. A store
# is its objects, each file named by its own hash and never rewritten, so two
# runs' appends are disjoint sets of new files and the merge cannot conflict;
# the store derives two tips from the union, and its next append joins them.
set -euo pipefail

message=$1
branch=roadmap-data

git add roadmap
if git diff --cached --quiet; then
  echo "roadmap-data: nothing to commit"
  exit 0
fi

git -c user.name='github-actions[bot]' \
    -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
    commit --quiet --message "$message"

for attempt in 1 2 3 4 5; do
  if git push --quiet origin "HEAD:$branch"; then
    echo "roadmap-data: pushed on attempt $attempt"
    exit 0
  fi
  sleep $((attempt * 2))
  git -c user.name='github-actions[bot]' \
      -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
      pull --quiet --no-rebase --no-edit origin "$branch"
done

echo "roadmap-data: could not push after 5 attempts" >&2
exit 1
