ghostty_commit := "83fb95624b0439ee660c2679b43b38c8b63769cd"
ghostty_repo := "joordih/krad-ade"
ghostty_release := "ghostty-vt-83fb9562"
ghostty_libs := "target/ghostty-vt"
ghostty_upstream := "https://github.com/ghostty-org/ghostty.git"
ghostty_source := "target/ghostty-src"
ghostty_musl_libs := "target/" + ghostty_release + "-musl"
zig_tools := "packaging/zig"
musl_targets := "x86_64-unknown-linux-musl aarch64-unknown-linux-musl"
conpty_version := "1.25.260930003"
conpty_dir := "target/conpty"

ui_source := "crates/krad-ui/src"
ui_coupled_crates := "krad_chat krad_repo krad_host krad_agents"

lints := "-D warnings -W clippy::pedantic -W clippy::unwrap_used -W clippy::expect_used -W clippy::panic -W clippy::dbg_macro -W clippy::print_stdout -W clippy::print_stderr -W clippy::todo -W clippy::undocumented_unsafe_blocks"

format:
    cargo fmt --all

format-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets -- {{lints}}

release-preflight: format-check lint
    cargo test --workspace

ui-coupling:
    @for crate in {{ui_coupled_crates}}; do printf '%-12s %s\n' "$crate" "$(grep -rlw "$crate" {{ui_source}} | wc -l | tr -d ' ')"; done
    @printf '%-12s %s\n' total "$(grep -rlw $(printf ' -e %s' {{ui_coupled_crates}}) {{ui_source}} | wc -l | tr -d ' ')"

ghostty-libs target=`rustc -vV | sed -n 's/^host: //p'`:
    mkdir -p {{ghostty_libs}}
    gh release download {{ghostty_release}} --repo {{ghostty_repo}} --pattern libghostty-vt-{{target}}.tar.gz --output - | tar -xz -C {{ghostty_libs}}

ghostty-libs-build source:
    packaging/ghostty-vt/build.sh {{source}} target/ghostty-vt-install
    packaging/ghostty-vt/package.sh target/ghostty-vt-install dist/ghostty-vt

ghostty-source:
    test -d {{ghostty_source}}/.git || git clone --config core.longpaths=true --filter=blob:none --no-checkout {{ghostty_upstream}} {{ghostty_source}}
    git -C {{ghostty_source}} cat-file -e {{ghostty_commit}}^{commit} || git -C {{ghostty_source}} fetch --filter=blob:none origin {{ghostty_commit}}
    git -C {{ghostty_source}} checkout --quiet --detach {{ghostty_commit}}

ghostty-libs-musl target: ghostty-source
    #!/bin/sh
    set -eu
    prefix={{justfile_directory()}}/{{ghostty_musl_libs}}/{{target}}
    [ -f "$prefix/share/pkgconfig/libghostty-vt-static.pc" ] && exit 0
    rm -rf "$prefix.partial"
    cd {{ghostty_source}}
    "${ZIG:-zig}" build -Demit-lib-vt=true -Doptimize=ReleaseFast -Dcpu=baseline -Demit-xcframework=false \
        -Dapp-runtime=none -Dtarget={{replace(target, "-unknown-", "-")}} --prefix "$prefix.partial"
    pc="$prefix.partial/share/pkgconfig/libghostty-vt-static.pc"
    sed 's|^prefix=.*|prefix=${pcfiledir}/../..|' "$pc" > "$pc.relocatable"
    mv "$pc.relocatable" "$pc"
    mv "$prefix.partial" "$prefix"

agents-build target: (ghostty-libs-musl target)
    rustup target add {{target}}
    env KRAD_ZIG_TARGET={{replace(target, "-unknown-", "-")}} \
        CC_{{replace(target, "-", "_")}}={{justfile_directory()}}/{{zig_tools}}/cc \
        AR_{{replace(target, "-", "_")}}={{justfile_directory()}}/{{zig_tools}}/ar \
        PKG_CONFIG_ALLOW_CROSS_{{replace(target, "-", "_")}}=1 \
        PKG_CONFIG_PATH_{{replace(target, "-", "_")}}={{justfile_directory()}}/{{ghostty_musl_libs}}/{{target}}/share/pkgconfig \
        CARGO_TARGET_{{uppercase(replace(target, "-", "_"))}}_LINKER=rust-lld \
        cargo build --release --locked -p krad-app --bin krad-ade-agents --no-default-features --features runner-runtime --target {{target}}

agents-linux:
    for target in {{musl_targets}}; do just agents-build $target; done

conpty:
    mkdir -p {{conpty_dir}}
    curl -fsSL -o {{conpty_dir}}/conpty.nupkg https://www.nuget.org/api/v2/package/Microsoft.Windows.Console.ConPTY/{{conpty_version}}
    unzip -o -j {{conpty_dir}}/conpty.nupkg runtimes/win-x64/native/conpty.dll build/native/runtimes/x64/OpenConsole.exe -d {{conpty_dir}}
