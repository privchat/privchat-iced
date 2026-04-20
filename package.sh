#!/bin/bash
# Usage: ./package.sh <profile>
# Example: ./package.sh loan
#          ./package.sh dubai

set -e

PROFILE="${1:?Usage: ./package.sh <profile>}"
VERSION="0.1.0"
ARCH="x86_64"

echo "==> Building privchat-iced with profile: ${PROFILE}"

# Ensure Strawberry Perl is in PATH for OpenSSL build
export PATH="/c/Users/admin/.cargo/bin:/c/Strawberry/perl/bin:/c/Strawberry/c/bin:$PATH"
export PRIVCHAT_PROFILE="${PROFILE}"

# Build release
cargo build --release

# Rename exe with profile name
EXE_SRC="target/release/privchat-iced.exe"
EXE_DST="target/release/privchat-iced-${PROFILE}.exe"
cp "${EXE_SRC}" "${EXE_DST}"
echo "==> EXE: ${EXE_DST}"

# Build MSI (requires WiX in PATH)
export PATH="/c/Users/admin/wix314:$PATH"
MSI_NAME="privchat-iced-${PROFILE}-${VERSION}-${ARCH}.msi"
MSI_DST="target/wix/${MSI_NAME}"

cargo wix --no-build --output "${MSI_DST}"
echo "==> MSI: ${MSI_DST}"

echo "==> Done!"
