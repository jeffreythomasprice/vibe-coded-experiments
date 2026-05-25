#!/usr/bin/env bash
# Build release binaries for Linux (native) and Windows (cross-compiled via
# cargo-xwin), and stage them in ./release/.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

LINUX_TARGET="x86_64-unknown-linux-gnu"
WIN_TARGET="x86_64-pc-windows-msvc"
RELEASE_DIR="release"

if ! command -v cargo-xwin >/dev/null 2>&1; then
    echo "error: cargo-xwin is not installed." >&2
    echo "       install with: cargo install cargo-xwin" >&2
    exit 1
fi

for target in "${LINUX_TARGET}" "${WIN_TARGET}"; do
    if ! rustup target list --installed | grep -qx "${target}"; then
        echo "error: rustc target ${target} is not installed." >&2
        echo "       install with: rustup target add ${target}" >&2
        exit 1
    fi
done

echo "==> Building Linux release (${LINUX_TARGET})"
cargo build --release --target "${LINUX_TARGET}"

echo "==> Building Windows release (${WIN_TARGET}, via cargo-xwin)"
cargo xwin build --release --target "${WIN_TARGET}"

echo "==> Staging binaries in ${RELEASE_DIR}/"
mkdir -p "${RELEASE_DIR}/linux-x86_64" "${RELEASE_DIR}/windows-x86_64"
cp "target/${LINUX_TARGET}/release/ecs" "${RELEASE_DIR}/linux-x86_64/ecs"
cp "target/${WIN_TARGET}/release/ecs.exe" "${RELEASE_DIR}/windows-x86_64/ecs.exe"

echo "==> Done."
ls -lh "${RELEASE_DIR}/"
