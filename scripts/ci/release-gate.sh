#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
[[ ${GITHUB_EVENT_NAME:-} == push && ${GITHUB_REF_TYPE:-} == tag ]] || {
    echo 'Release requires a tag push' >&2; exit 1;
}
tag=${RELEASE_TAG:?Release tag required}
number='(0|[1-9][0-9]*)'
identifier="($number|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)"
semver="^v$number\\.$number\\.$number(-$identifier(\\.$identifier)*)?(\\+[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?$"
[[ $tag =~ $semver && ${REF_PROTECTED:-false} == true ]] || {
    echo 'Release requires a valid protected semantic-version tag' >&2; exit 1;
}
# This project's package version is an explicit string, not a workspace value.
version=$(sed -nE '/^\[package\]/,/^\[/s/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' osmocore-rs/Cargo.toml)
[[ -n $version && ${tag#v} == "$version" ]] || {
    echo 'Tag and Cargo.toml versions differ' >&2; exit 1;
}
branch=${RELEASE_BRANCH:?Approved release branch required}
git check-ref-format "refs/heads/$branch"
git fetch --no-tags origin "+refs/heads/$branch:refs/remotes/origin/$branch"
commit=$(git rev-parse HEAD)
tagged=$(git rev-parse "refs/tags/$tag^{commit}")
[[ $commit == "$tagged" && $commit == "${GITHUB_SHA:?Workflow revision required}" ]] || {
    echo 'Checked-out revision must match the tagged workflow commit' >&2; exit 1;
}
git merge-base --is-ancestor "$commit" "refs/remotes/origin/$branch"
printf 'Authorized release candidate %s: %s\n' "$tag" "$commit"
