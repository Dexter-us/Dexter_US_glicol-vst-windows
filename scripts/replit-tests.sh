#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Replit's Nix package setup discovers lib/pkgconfig, but Xorg protocol
# definitions live in share/pkgconfig. Add one installed protocol directory.
for candidate in /nix/store/*xorgproto*/share/pkgconfig; do
    if [[ -f "$candidate/xproto.pc" ]]; then
        export PKG_CONFIG_PATH="$candidate:${PKG_CONFIG_PATH:-}"
        break
    fi
done
# Older xcb crates link by library name without emitting Cargo search paths.
for package in xcb xcb-icccm x11 gl xcursor; do
    directory="$(pkg-config --variable=libdir "$package")"
    export LIBRARY_PATH="$directory:${LIBRARY_PATH:-}"
    export LD_LIBRARY_PATH="$directory:${LD_LIBRARY_PATH:-}"
done
cargo test --locked
