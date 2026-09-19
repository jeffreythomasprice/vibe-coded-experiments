#!/usr/bin/env bash
# Fetches the stable-diffusion.cpp/ subtree that diffusion-rs-sys 0.1.20 bundles.
# Run this once after cloning, before `cargo build`.
#
# We vendor a patched diffusion-rs-sys (see build.rs's `.opaque_type("_IO_FILE")`
# call, added to work around a bindgen/glibc incompatibility on newer glibc), but
# don't commit its ~270MB vendored C++ source tree to git. This re-downloads just
# that tree from the exact same crates.io release instead.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

VERSION=0.1.20
TARBALL="diffusion-rs-sys-${VERSION}.crate"

curl -sL -H "User-Agent: diffusion-rs-sys-vendor-fetch" \
    "https://crates.io/api/v1/crates/diffusion-rs-sys/${VERSION}/download" \
    -o "$TARBALL"

tar xzf "$TARBALL" "diffusion-rs-sys-${VERSION}/stable-diffusion.cpp"
rm -rf stable-diffusion.cpp
mv "diffusion-rs-sys-${VERSION}/stable-diffusion.cpp" .
rmdir "diffusion-rs-sys-${VERSION}"
rm "$TARBALL"
