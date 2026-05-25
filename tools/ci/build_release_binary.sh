#!/usr/bin/env bash
set -euo pipefail

binary_name="${1:-${RELEASE_BINARY:-cloakpipe}}"

cargo build --release -p cloakpipe-cli --bin "${binary_name}"
