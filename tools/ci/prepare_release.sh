#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../.." && pwd)"

dry_run=0
skip_push=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      dry_run=1
      ;;
    --skip-push)
      skip_push=1
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
  shift
done

cd "${repo_root}"

github_output_args=()
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  github_output_args=(--github-output "${GITHUB_OUTPUT}")
fi

if (( dry_run )); then
  version="$(python3 tools/ci/bump_workspace_version.py "${github_output_args[@]}")"
  echo "version=${version}"
  echo "tag=v${version}"
  exit 0
fi

version="$(python3 tools/ci/bump_workspace_version.py --write "${github_output_args[@]}")"
tag="v${version}"

git add Cargo.toml crates/*/Cargo.toml
git commit -m "chore(release): bump version to ${tag}"

if (( ! skip_push )); then
  git push origin HEAD:main
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "sha=$(git rev-parse HEAD)" >> "${GITHUB_OUTPUT}"
fi
