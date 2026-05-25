#!/usr/bin/env bash
set -euo pipefail

package_name="${1:-cloakpipe-cli}"
version="${2:-${RELEASE_VERSION:-}}"
asset_name="${3:-${CARGO_PACKAGE_ASSET_NAME:-borgius-cloakpipe}}"

if [[ -z "${version}" ]]; then
  echo "usage: $0 [package_name] [version] [asset_name]" >&2
  exit 1
fi

cargo package --locked --workspace

crate_path="target/package/${package_name}-${version}.crate"
asset_path="dist/${asset_name}-${version}.crate"

if [[ ! -f "${crate_path}" ]]; then
  echo "missing packaged crate: ${crate_path}" >&2
  exit 1
fi

mkdir -p dist
find target/package -maxdepth 1 -type f -name '*.crate' -exec cp {} dist/ \;
cp "${crate_path}" "${asset_path}"
