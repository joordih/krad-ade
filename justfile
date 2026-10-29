ghostty_commit := "83fb95624b0439ee660c2679b43b38c8b63769cd"
ghostty_repo := "joordih/krad-ade"
ghostty_release := "ghostty-vt-83fb9562"
ghostty_libs := "target/ghostty-vt"
conpty_version := "1.25.260930003"
conpty_dir := "target/conpty"

lints := "-D warnings -W clippy::pedantic -W clippy::unwrap_used -W clippy::expect_used -W clippy::panic -W clippy::dbg_macro -W clippy::print_stdout -W clippy::print_stderr -W clippy::todo -W clippy::undocumented_unsafe_blocks"

format:
    cargo fmt --all

format-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets -- {{lints}}

release-preflight: format-check lint
    cargo test --workspace

ghostty-libs target=`rustc -vV | sed -n 's/^host: //p'`:
    mkdir -p {{ghostty_libs}}
    gh release download {{ghostty_release}} --repo {{ghostty_repo}} --pattern libghostty-vt-{{target}}.tar.gz --output - | tar -xz -C {{ghostty_libs}}

ghostty-libs-build source:
    packaging/ghostty-vt/build.sh {{source}} target/ghostty-vt-install
    packaging/ghostty-vt/package.sh target/ghostty-vt-install dist/ghostty-vt

conpty:
    mkdir -p {{conpty_dir}}
    curl -fsSL -o {{conpty_dir}}/conpty.nupkg https://www.nuget.org/api/v2/package/Microsoft.Windows.Console.ConPTY/{{conpty_version}}
    unzip -o -j {{conpty_dir}}/conpty.nupkg runtimes/win-x64/native/conpty.dll build/native/runtimes/x64/OpenConsole.exe -d {{conpty_dir}}
