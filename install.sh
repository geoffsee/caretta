#!/usr/bin/env bash
# Copyright (c) 2026 Geoff Seemueller
#
# Licensed under the MIT License or Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# See LICENSE-MIT or LICENSE-APACHE for the full license text.
#
# Additionally, this file is subject to the Revenue Sharing Agreement terms
# as defined in REVENUE-SHARING.md for covered organizations.
set -euo pipefail

REPO="geoffsee/caretta"
BINARY="caretta"
INSTALL_DIR="${CARETTA_INSTALL_DIR:-$HOME/.local/bin}"

# ── colors ──────────────────────────────────────────────────────────
bold="\033[1m"
dim="\033[2m"
green="\033[32m"
cyan="\033[36m"
red="\033[31m"
reset="\033[0m"

info()  { printf "${cyan}>${reset} %s\n" "$*"; }
ok()    { printf "${green}✓${reset} %s\n" "$*"; }
err()   { printf "${red}✗${reset} %s\n" "$*" >&2; }
die()   { err "$@"; exit 1; }

# ── detect platform ─────────────────────────────────────────────────
detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux*)  os="linux" ;;
    Darwin*) os="macos" ;;
    *)       die "Unsupported OS: $os" ;;
  esac

  case "$arch" in
    x86_64|amd64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)             die "Unsupported architecture: $arch" ;;
  esac

  echo "${arch}-${os}"
}

# ── latest release tag ──────────────────────────────────────────────
get_latest_version() {
  local url="https://api.github.com/repos/${REPO}/releases/latest"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/'
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$url" | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/'
  else
    die "Neither curl nor wget found — install one and try again"
  fi
}

# ── download ────────────────────────────────────────────────────────
download() {
  local url="$1" dest="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$dest"
  else
    wget -qO "$dest" "$url"
  fi
}

# ── main ────────────────────────────────────────────────────────────
main() {
  printf "\n${bold}  caretta installer${reset}\n\n"

  local platform version artifact url tmpdir

  platform="$(detect_platform)"
  info "Detected platform: ${bold}${platform}${reset}"

  version="$(get_latest_version)"
  [ -z "$version" ] && die "Could not determine latest release"
  info "Latest release:    ${bold}${version}${reset}"

  artifact="${BINARY}-${platform}.tar.gz"
  url="https://github.com/${REPO}/releases/download/${version}/${artifact}"
  info "Downloading ${dim}${url}${reset}"

  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT

  download "$url" "$tmpdir/$artifact"
  tar xzf "$tmpdir/$artifact" -C "$tmpdir"

  mkdir -p "$INSTALL_DIR"
  mv "$tmpdir/$BINARY" "$INSTALL_DIR/$BINARY"
  chmod +x "$INSTALL_DIR/$BINARY"

  ok "Installed ${bold}${BINARY}${reset} to ${INSTALL_DIR}/${BINARY}"

  # check PATH
  if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    printf "\n${dim}  Add it to your PATH:${reset}\n"
    printf "    ${bold}export PATH=\"%s:\$PATH\"${reset}\n" "$INSTALL_DIR"
    printf "${dim}  Add that line to ~/.bashrc or ~/.zshrc to make it permanent.${reset}\n"
  fi

  printf "\n  Run ${bold}caretta --help${reset} to get started.\n\n"
}

main