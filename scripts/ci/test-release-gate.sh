#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
mkdir -p "$temporary/scripts/ci" "$temporary/osmocore-rs"
cp "$root/scripts/ci/release-gate.sh" "$temporary/scripts/ci/"
printf '[package]\nversion = "0.1.0"\n' > "$temporary/osmocore-rs/Cargo.toml"
cd "$temporary"
git init --quiet -b main
git config user.name 'Pipeline gate test'
git config user.email pipeline@example.invalid
git add .
git -c core.hooksPath=/dev/null commit --quiet -m 'Approved release branch'
git remote add origin "$temporary"
git tag v0.1.0
export GITHUB_EVENT_NAME=push GITHUB_REF_TYPE=tag RELEASE_TAG=v0.1.0
export REF_PROTECTED=true RELEASE_BRANCH=main
GITHUB_SHA=$(git rev-parse HEAD)
export GITHUB_SHA
gate=$temporary/scripts/ci/release-gate.sh
"$gate"
reject() {
    if env "$@" "$gate" > /dev/null 2>&1; then
        printf 'Release gate unexpectedly accepted: %s\n' "$*" >&2
        exit 1
    fi
}
reject GITHUB_EVENT_NAME=pull_request
reject GITHUB_EVENT_NAME=workflow_dispatch
reject GITHUB_REF_TYPE=branch
reject REF_PROTECTED=false
reject GITHUB_SHA=incorrect
for tag in 0.1.0 v01.1.0 v0.1 v0.1.0-01 v0.1.0- v0.2.0; do
    reject "RELEASE_TAG=$tag"
done
git checkout --quiet -b unapproved
git -c core.hooksPath=/dev/null commit --quiet --allow-empty -m 'Off-branch release'
git tag -f v0.1.0
GITHUB_SHA=$(git rev-parse HEAD)
export GITHUB_SHA
reject RELEASE_TAG=v0.1.0
echo 'Release gate rejection checks passed'
