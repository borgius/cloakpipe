#!/bin/sh
set -eu

REPO="borgius/cloakpipe"
BINARY_NAME="cloakpipe"

log() {
  printf '%s\n' "$*"
}

preinstall_bundled_presets() {
  binary_path="$1"

  output="$("$binary_path" presets install 2>&1)" || {
    log "Warning: could not preinstall bundled presets. Run '${binary_path} presets install' manually."
    return 0
  }

  if [ -n "$output" ]; then
    printf '%s\n' "$output"
  fi
  log "Initialized global ${BINARY_NAME} home and bundled presets"
  return 0
}

resolve_installed_binary() {
  if command -v "$BINARY_NAME" >/dev/null 2>&1; then
    command -v "$BINARY_NAME"
    return 0
  fi

  cargo_home="${CARGO_HOME:-${HOME}/.cargo}"
  candidate="${cargo_home}/bin/${BINARY_NAME}"
  if [ -x "$candidate" ]; then
    printf '%s\n' "$candidate"
    return 0
  fi

  return 1
}

download_latest_tag() {
  if [ -n "${CLOAKPIPE_VERSION:-}" ]; then
    printf '%s\n' "$CLOAKPIPE_VERSION"
    return 0
  fi

  curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1
}

detect_platform() {
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"

  case "$os" in
    linux|darwin) ;;
    *)
      return 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)
      return 1
      ;;
  esac

  printf '%s %s\n' "$os" "$arch"
}

pick_asset_url() {
  release_json="$1"
  os="$2"
  arch="$3"

  printf '%s\n' "$release_json" \
    | sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | grep -i "${os}" \
    | grep -i "${arch}" \
    | grep -i "${BINARY_NAME}" \
    | grep -E '\.(tar\.gz|tgz|zip)$' \
    | head -n 1
}

install_binary() {
  binary_path="$1"
  target_dir="${INSTALL_DIR:-}"

  if [ -z "$target_dir" ]; then
    if [ -w "/usr/local/bin" ]; then
      target_dir="/usr/local/bin"
    elif command -v sudo >/dev/null 2>&1; then
      target_dir="/usr/local/bin"
      sudo install -m 755 "$binary_path" "${target_dir}/${BINARY_NAME}"
      log "Installed ${BINARY_NAME} to ${target_dir}/${BINARY_NAME}"
      preinstall_bundled_presets "${target_dir}/${BINARY_NAME}"
      return 0
    else
      target_dir="${HOME}/.local/bin"
    fi
  fi

  mkdir -p "$target_dir"
  install -m 755 "$binary_path" "${target_dir}/${BINARY_NAME}"
  log "Installed ${BINARY_NAME} to ${target_dir}/${BINARY_NAME}"
  preinstall_bundled_presets "${target_dir}/${BINARY_NAME}"
}

install_from_release() {
  platform="$(detect_platform)" || return 1
  os="$(printf '%s' "$platform" | awk '{print $1}')"
  arch="$(printf '%s' "$platform" | awk '{print $2}')"

  tag="$(download_latest_tag)"
  [ -n "$tag" ] || return 1

  release_json="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/tags/${tag}")" || return 1
  asset_url="$(pick_asset_url "$release_json" "$os" "$arch")"
  [ -n "$asset_url" ] || return 1

  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT INT TERM

  archive_path="${tmpdir}/artifact"
  curl -fsSL "$asset_url" -o "$archive_path"

  case "$asset_url" in
    *.tar.gz|*.tgz)
      tar -xzf "$archive_path" -C "$tmpdir"
      ;;
    *.zip)
      if ! command -v unzip >/dev/null 2>&1; then
        return 1
      fi
      unzip -q "$archive_path" -d "$tmpdir"
      ;;
    *)
      return 1
      ;;
  esac

  binary_path="$(find "$tmpdir" -type f -name "$BINARY_NAME" | head -n 1)"
  [ -n "$binary_path" ] || return 1

  install_binary "$binary_path"
  return 0
}

install_with_cargo() {
  binary_path=""
  git_repo_url="https://github.com/${REPO}.git"
  tag="$(download_latest_tag || true)"

  if ! command -v cargo >/dev/null 2>&1; then
    log "cargo is required for fallback install, but it was not found."
    return 1
  fi

  if [ -n "$tag" ]; then
    cargo install --locked --git "$git_repo_url" --tag "$tag" --bin "${BINARY_NAME}" cloakpipe-cli
    log "Installed ${BINARY_NAME} with cargo install --locked --git ${REPO} --tag ${tag} cloakpipe-cli"
  else
    cargo install --locked --git "$git_repo_url" --bin "${BINARY_NAME}" cloakpipe-cli
    log "Installed ${BINARY_NAME} with cargo install --locked --git ${REPO} cloakpipe-cli"
  fi

  binary_path="$(resolve_installed_binary || true)"
  [ -n "$binary_path" ] && preinstall_bundled_presets "$binary_path"
}

main() {
  if [ "${CLOAKPIPE_FORCE_CARGO:-0}" = "1" ]; then
    log "CLOAKPIPE_FORCE_CARGO=1 set, skipping release install."
    install_with_cargo
    return 0
  fi

  if install_from_release; then
    log "Release install complete."
    return 0
  fi

  log "Release install failed, falling back to cargo install..."
  install_with_cargo
}

main "$@"
