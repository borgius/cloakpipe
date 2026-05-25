#!/usr/bin/env bash
set -euo pipefail

binary_name="${1:-${RELEASE_BINARY:-cloakpipe}}"
version="${2:-${RELEASE_VERSION:-}}"
os_name="${3:-${RELEASE_OS:-}}"
arch="${4:-${RELEASE_ARCH:-}}"

if [[ -z "${version}" || -z "${os_name}" || -z "${arch}" ]]; then
  echo "usage: $0 [binary_name] [version] [os] [arch]" >&2
  exit 1
fi

archive_name="${binary_name}-${version}-${os_name}-${arch}"

rm -rf "dist/${archive_name}" "dist/${archive_name}.tar.gz"
mkdir -p "dist/${archive_name}"
cp "target/release/${binary_name}" "dist/${archive_name}/${binary_name}"
cp -R policies "dist/${archive_name}/policies"
tar -C dist -czf "dist/${archive_name}.tar.gz" "${archive_name}"
