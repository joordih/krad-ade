#!/bin/sh
# Builds the static libghostty-vt for every platform the app ships on, from
# a Ghostty checkout at the commit our libghostty-rs fork pins, with Zig
# (ADR-0024). Zig cross-compiles, so one machine builds them all.
#
# Usage: packaging/ghostty-vt/build.sh <ghostty-checkout> <install-root>
set -eu

source_dir=$1
# Zig runs from inside the checkout, so a relative install root would land
# there instead of where the caller asked.
mkdir -p "$2"
install_root=$(cd "$2" && pwd)
zig=${ZIG:-zig}

for zig_target in x86_64-linux-gnu x86_64-windows-gnu aarch64-macos x86_64-macos; do
    (cd "$source_dir" && "$zig" build \
        -Demit-lib-vt=true \
        -Doptimize=ReleaseFast \
        -Dcpu=baseline \
        -Demit-xcframework=false \
        -Dapp-runtime=none \
        -Dtarget="$zig_target" \
        --prefix "$install_root/$zig_target")
done
