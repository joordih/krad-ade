#!/bin/sh
# Packs what build.sh installed into one archive per Rust target, to publish
# as release assets that `just ghostty-libs` fetches (ADR-0024). The
# pkg-config file is made relocatable, so the archive works wherever it is
# unpacked.
#
# Usage: packaging/ghostty-vt/package.sh <install-root> <out-dir>
set -eu

install_root=$1
out_dir=$2
zig=${ZIG:-zig}
mkdir -p "$out_dir"

# Every target installs the same headers; a target whose install stopped
# after its library takes them from one that has them.
headers=$(find "$install_root" -path '*/include/ghostty/vt.h' | head -n 1)
headers_dir=$(dirname "$(dirname "$headers")")

for pair in \
    x86_64-linux-gnu:x86_64-unknown-linux-gnu \
    x86_64-windows-gnu:x86_64-pc-windows-gnu \
    aarch64-macos:aarch64-apple-darwin \
    x86_64-macos:x86_64-apple-darwin; do
    zig_target=${pair%%:*}
    rust_target=${pair#*:}
    from="$install_root/$zig_target"
    [ -d "$from/lib" ] || continue
    stage=$(mktemp -d)
    mkdir -p "$stage/lib" "$stage/include" "$stage/share/pkgconfig"
    find "$from/lib" -maxdepth 1 -type f \( -name 'libghostty-vt.a' -o -name 'ghostty-vt-static.lib' \) \
        -exec cp {} "$stage/lib/" \;
    # Zig bundles its compiler_rt, whose stack probes are weak COFF symbols
    # MinGW's ld leaves unresolved (undefined ___chkstk_ms); libgcc and the C
    # runtime provide everything the library takes from it.
    if [ "$rust_target" = x86_64-pc-windows-gnu ]; then
        "$zig" ar t "$stage/lib/ghostty-vt-static.lib" | grep 'compiler_rt\.obj$' |
            while read -r member; do
                "$zig" ar d "$stage/lib/ghostty-vt-static.lib" "$member"
            done
    fi
    cp -R "$headers_dir/ghostty" "$stage/include/"
    sed 's|^prefix=.*|prefix=${pcfiledir}/../..|' \
        "$from/share/pkgconfig/libghostty-vt-static.pc" \
        > "$stage/share/pkgconfig/libghostty-vt-static.pc"
    tar -czf "$out_dir/libghostty-vt-$rust_target.tar.gz" -C "$stage" .
    rm -r "$stage"
done
