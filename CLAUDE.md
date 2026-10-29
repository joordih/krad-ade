# CLAUDE.md

Engineering guide for agents working on krad-ade, a desktop ADE (GPUI) to run many coding agents
at once, as chats or terminals, organised in sessions. `AGENTS.md` points here; this is the single source of these rules.

## Architecture

Virtual workspace: the root `Cargo.toml` has no `[package]`, only `[workspace]`,
`[workspace.package]`, `[workspace.dependencies]` and the dev profile overrides. One responsibility
per crate:

| Crate | Responsibility |
|---|---|
| `krad-domain` | Pure domain: ids, environments, groups, sessions and their panes, agents and harnesses, notes, settings, the review types every code host reads into, and the persisted layout format. No GPUI or terminal types and no I/O. |
| `krad-pty` | Process spawning and PTY I/O over `portable-pty` (Unix PTY, Windows ConPTY) behind a platform-neutral API; platform shells in `unix.rs`, `windows.rs`, `wsl.rs`. |
| `krad-term` | Terminal emulation over libghostty-vt, linked from prebuilt static libraries. Bytes in, renderable snapshots out, key and mouse input encoded. |
| `krad-session` | Glue: owns a PTY + terminal pair, spawn/reader/writer threads, emits session events (`PumpMsg`). Re-exports the PTY and terminal items `krad-ui` needs, so the UI never depends on `krad-pty` or `krad-term` directly. |
| `krad-store` | The app's files on disk: config folder lookup, the layout file and its writer thread, the chat transcripts and their writer thread, `settings.json`, the saved Claude Code logins and the GitLab and GitHub logins (system keyring, config-folder fallback). Synchronous. |
| `krad-repo` | A Group's root folder: directory listing, the file index behind `@` mentions, text file reads, git status and worktrees through the `git` CLI. Synchronous; the UI calls it from the background executor. |
| `krad-chat` | Headless agent turns and Claude Code's live chat process with its message queue: the non-interactive command line per harness, the prompt, the Claude Code skills a message can call, and the parsed output stream of each turn. |
| `krad-gitlab` | GitLab's REST API v4: OAuth sign-in (Authorization Code with PKCE through a loopback redirect, Device Authorization Grant as fallback) and token refresh, merge requests, commits, changes, discussions, notes, approvals, merge, pipelines, jobs, branches and repository tree and files, over `ureq`. Synchronous; the UI calls it from the background executor. |
| `krad-github` | GitHub's REST API, with GraphQL for review threads: device-flow sign-in, pull requests, commits, changed files, conversation comments, reviews, review threads, approvals, merge, Actions runs and jobs, branches and repository files, read into the review types of `krad-domain`, over `ureq`. Synchronous; the UI calls it from the background executor. |
| `krad-agents` | Agent coordination bus, after Orca's orchestration: the loopback endpoint the app serves, its line-JSON protocol, the mailbox, and the `krad-ade-agents` CLI agents run from their shell. |
| `krad-ui` | All GPUI views, elements, actions, the per-platform keymap (`keymap.rs`: Cmd on macOS, Ctrl+Shift on Windows/Linux so shells keep their control keys) and the light/dark theme. |
| `krad-app` | Binaries: `krad-ade` (bootstrap, wiring, window creation; kept thin) and the console `krad-ade-agents` CLI, which only calls `krad-agents`. |

**Dependency rules (mandatory):**

- `krad-app -> krad-ui -> {krad-session -> {krad-pty, krad-term}, krad-repo, krad-store, krad-agents, krad-chat, krad-gitlab, krad-github}`,
  `krad-app -> krad-agents` and `krad-app -> krad-store`. Any crate may depend on `krad-domain`. No other edges, no cycles.
- `krad-domain` depends on no workspace crate and on none of `gpui`, `libghostty-vt` or `tokio`.
- `krad-term` must not depend on `krad-pty`; it is tested by feeding raw bytes.
- `gpui` (published as `gpui-pre`, pulled in by `gpui-kit`) is only allowed in `krad-ui` and
  `krad-app`.

Inside `krad-ui`, the root view lives in `environment_shell.rs` and each concern of it (sessions
and their panes, chat surfaces, terminal bodies and actions, sidebar, groups, file tree, file
tabs, notes, review, side panel, utilities panel, settings, agent bus, agent hooks, context menus,
dialogs, palette, input, terminal lifecycle, layout snapshot, window close, arcade) is a child
module in
`environment_shell/` with its own `impl EnvironmentShell` block. Children see the shell's private
fields; widen only to `pub(super)` what siblings call. Concerns split further into their own child
modules widen to `pub(in crate::environment_shell)` only what the rest of the shell calls.

## Domain and Decisions

- Use the vocabulary in `CONTEXT.md` for type, module and function names.
- Design decisions may be recorded as ADRs in `docs/adr/`, which is kept local and out of the
  repository; when it exists, respect them, and if a change would contradict one, stop and ask.

## Rust Style and Idioms

### Modules and visibility

- **`foo.rs` + `foo/` for submodules, never `foo/mod.rs`.**
- **`lib.rs` wires, it does not implement.** In split crates (`krad-domain`, `krad-ui`) it only
  declares modules and re-exports the public API. Small crates (`krad-pty`, `krad-term`,
  `krad-session`) keep their core type and API in `lib.rs`.
- **File size.** Propose a split once a file passes about 400 lines or mixes responsibilities;
  split by domain into sibling modules instead of growing it.
- **Small public surface.** Private by default, `pub(crate)` for crate internals, `pub` only for
  what another crate uses. `#[must_use]` on functions and types whose result must not be dropped.
- **One definition per type.** When two crates need the same enum or struct, define it in the lower
  crate and re-export it from the higher one (as `krad-session` does for `krad-ui`). Never mirror a
  type across crates or round-trip through strings (`parse(&id.to_string())`).

### Naming and documentation

- Target the edition in the root `Cargo.toml` and the toolchain pinned in `rust-toolchain.toml`
  (a nightly, for rustfmt). Do not change either without asking.
- `snake_case` for functions, modules and variables, `UpperCamelCase` for types and traits,
  `SCREAMING_SNAKE_CASE` for constants. No `I` prefixes on traits, no `Impl`, `Manager` or
  `Helper` suffixes.
- Public items carry `///` only for the `# Errors`, `# Panics` and `# Safety` sections clippy
  requires; no other rustdoc.
- Identifiers, file names and every user-facing string (labels, tooltips, toasts, dialogs) are in
  English.

### Types and modelling

- **Model the domain with the type system**: newtypes for identifiers and units (pixels, cells,
  zoom factor), enums instead of boolean flags or string states, `Option`/`Result` instead of
  sentinel values. No `Box<dyn Any>` or stringly typed APIs unless unavoidable.
- **Parse at the boundaries**: validate when constructing a type, so values inside the domain are
  valid by construction.
- **Match on types, never strings.** Strings appear only at serialization, display and CLI
  boundaries (`FromStr`/`Display`).
- **Concrete types over `serde_json::Value`** wherever the shape is known. `Value` is only for
  formats we do not own (vendor hook payloads, other tools' settings files), parsed into our types
  at that boundary. Structured or persisted data is structs and enums with `serde` derives, never
  loose maps with string keys.
- **Derive, don't hand-write**: `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `Default` (when every
  field has a sensible default) and `serde`.
- **Conversions through `From`/`Into`/`TryFrom`/`TryInto`/`FromStr`.** Ask before adding an ad hoc
  `to_x`/`from_x` path.
- **No magic values.** Sizes, colors, timings, zoom limits, keybindings, keys and labels are named
  `const` items, enum variants or configuration values.
- **Paths** are `Path`/`PathBuf`, never joined strings. Windows and WSL path translation lives in
  one place.
- **Time** is `std::time::{Duration, Instant}` with named constants; no epoch arithmetic or bare
  numbers like `86400`. Ask before adding a date crate.

### Idiomatic Rust, no Java patterns

- **No getters and setters by default.** Public or `pub(crate)` fields when there is no invariant to
  protect; methods only when they enforce one.
- **Constructors:** `new` for the common case, `Default` for sensible defaults, `with_*` or a
  builder only when there are many optional fields.
- **Composition over inheritance.** A trait with default methods where Java would use an abstract
  class; an `enum` with `match` when the set of variants is closed.
- **Traits mark behaviour boundaries**, and only with two real implementations or an I/O seam tests
  replace. Generics for hot paths (rendering, PTY pumping); `Arc<dyn Trait>` where generics would
  spread through the app. No interfaces "just in case".
- **Manual dependency injection:** pass dependencies through constructors and wire them in
  `krad-app`. No DI frameworks, service locators or global mutable singletons.
- **Free functions are fine**; not everything is a method.
- **Borrow in, own out:** take `&str`, `&[T]`, `&Path`, `impl AsRef<Path>`; return owned values.
- **Ownership over cloning.** Do not sprinkle `.clone()`, `Rc<RefCell<_>>` or `Arc<Mutex<_>>` to
  silence the borrow checker; redesign (ids plus maps, message passing, GPUI entities) and explain
  the choice.
- **Forbidden: `Mutex<()>` / `Arc<Mutex<()>>`.** A lock guards the state it protects.
- **Guard clauses** instead of nested `if` blocks; keep the happy path unindented.
- **Iterators and combinators** over index loops when they read better; `Cow<'_, str>` when an
  allocation is only sometimes needed.

### Errors

- `thiserror` enums per crate or module boundary in library and core crates; `anyhow` only in
  `krad-app` and top-level task handlers.
- Propagate with `?` and add context when crossing a layer. Never silence a `Result` that matters
  with `let _ = …` or `.ok()`.
- **No `.unwrap()`, `.expect()` or `panic!` in production code.** Use `?`, `ok_or_else`,
  `unwrap_or_default`, `unwrap_or_else(PoisonError::into_inner)` for poisoned locks. `expect` is
  allowed only where the invariant is proven locally, and its message states it. Tests may unwrap.

### Concurrency and I/O

- **Never block the UI thread.** PTY reads and writes, process spawning, file and network I/O go
  through the background executor or dedicated threads.
- **Never `block_on` on the UI thread or inside async code.** `futures::executor::block_on` is only
  for handing a message to a channel from a dedicated thread (the PTY pump, the agents bus
  listener).
- **Run independent work concurrently** (`futures::join!`, several background tasks) instead of one
  after another.
- **Stream, don't buffer.** PTY output, agent output and process pipes are consumed as they arrive
  through bounded channels.
- **Crates over subprocesses**, unless a decision says otherwise: `git` and `wsl.exe` run through
  their CLIs on purpose.

### GPUI and performance

- Follow the GPUI Kit coding guides (https://gpui-kit.com/docs/coding-guides) for entity ownership,
  `Context` usage and view structure.
- Keep GPUI types out of `krad-domain`, `krad-term` and `krad-session`; convert domain and snapshot
  types to GPUI types inside `krad-ui`.
- Keep `render` and `paint` free of avoidable allocations, I/O and locking; prefer cached or
  incremental work over recomputing per frame.
- Never claim a performance improvement without a measurement (FPS monitor, benchmark or trace)
  before and after.

### Unsafe and platform code

- No `unsafe` unless there is no safe alternative; every `unsafe` block has a `// SAFETY:` comment
  explaining why the invariants hold.
- Platform-specific code (Windows ConPTY, WSL bridging, custom titlebar and window controls) sits
  behind `#[cfg(...)]` in dedicated modules with a shared, platform-neutral interface. PTY platform
  code lives in `krad-pty` (`unix.rs`, `windows.rs`, `wsl.rs`); ConPTY itself comes from
  `portable-pty`. No scattered `cfg` attributes in domain or UI logic.
- When a change touches platform code, say in the final summary on which platforms it was compiled
  and tested.

### Tracing and logging

- Use `tracing`, never `println!`/`eprintln!`, outside tests and the CLI's own output. `tracing` is
  not yet a workspace dependency: ask before adding it.
- `#[tracing::instrument(skip_all, fields(...))]` on functions that cross a thread or process
  boundary (spawning a PTY, starting an agent, running `git`, serving a bus request), recording
  ids, never terminal contents.
- Levels:
  - `error!` — unrecoverable for that operation.
  - `warn!` — unexpected but recoverable.
  - `info!` — lifecycle milestones (environment opened, agent started, session ended).
  - `debug!` — diagnostics, including expected states such as a harness not found on `PATH` (not a
    warning).
  - `trace!` — per-chunk or per-frame detail.

### Secrets and configuration

- Never hardcode API keys, tokens, absolute user paths or environment URLs; read them from
  environment variables or the existing configuration.
- Never log, `Debug`-print or put in an error message a token, an API key, an environment value,
  terminal contents or personal data. Types holding a secret (the agents bus token) implement
  `Debug` by hand and print `[REDACTED]`, exposing the value only where it is used.

## Code Quality

- **Never run `cargo fmt` on stable in this repo.** Always use the pinned nightly rustfmt
  (`just format`, `just format-check`, or plain `cargo fmt` — `rust-toolchain.toml` selects the
  right nightly automatically).

```bash
just format              # Format Rust (pinned nightly)
just format-check        # CI format check
just lint                # clippy with the lint set below, every crate
just release-preflight   # fmt + clippy gates
cargo check              # Fast compile check
taplo fmt                # Format TOML files
```

- Readable on its own: clarity comes from names, small focused functions, clear types and simple
  structure, never from comments.
- No comments anywhere: not in Rust, TOML, the `justfile` or the workflows. The only exceptions
  are the `// SAFETY:` comment every `unsafe` block needs and the `/// # Errors`, `/// # Panics`
  and `/// # Safety` sections clippy requires.
- No commented-out code, unused imports, `#[allow(dead_code)]` or dead code.
- Do not copy the style of worse existing code: new code and every block you modify follow these
  rules. Do not reformat or fix code outside the task; style changes stay in the functions or blocks
  you touch.
- **Cleaning up existing code.** Fix magic values and separator comments only inside pending
  changes: uncommitted (staged and unstaged) changes, untracked files and local commits not yet
  pushed. Find them with `git diff @{upstream} --name-only` (or `git diff HEAD --name-only` without
  an upstream) and `git ls-files --others --exclude-standard`, and touch only the hunks those
  changes contain.

### Tests

- Behaviour changes add or update tests: unit tests in `#[cfg(test)] mod tests` next to the code,
  cross-crate behaviour in the crate's `tests/`, and GPUI interaction tests with the GPUI test
  context when UI behaviour changes.
- Domain logic (mosaic layout, viewport transforms, commands) is testable without a window;
  terminal behaviour is tested by feeding raw ANSI bytes to `krad-term`.
- Run only the relevant tests (`cargo test -p <crate> <test_name>`) unless asked for the full
  suite.

### Verification

After modifying `.rs` files, for each crate touched (this is what `just format-check` and
`just lint` run):

```bash
cargo fmt -p <crate> -- --check
cargo clippy -p <crate> --all-targets -- -D warnings -W clippy::pedantic \
  -W clippy::unwrap_used -W clippy::expect_used -W clippy::panic \
  -W clippy::dbg_macro -W clippy::print_stdout -W clippy::print_stderr \
  -W clippy::todo -W clippy::undocumented_unsafe_blocks
cargo test -p <crate> <test_name>
```

Fix warnings on the lines you changed; warnings in untouched code are out of scope.

After modifying any `Cargo.toml`, run `taplo fmt` and `cargo tree -i gpui-pre -e normal --workspace`,
and confirm `gpui-pre` only reaches `krad-ui` and `krad-app`.

### Checklist

**Run before every commit:**

- [ ] No secrets, tokens or personal paths (CRITICAL)
- [ ] `taplo fmt` (TOML changes)
- [ ] Rust fmt passes with the pinned nightly (`just format-check`)
- [ ] `just lint` passes
- [ ] `just release-preflight` passes
- [ ] Changed and added Rust tests pass; behaviour changes have tests
- [ ] No `unwrap`/`expect`/`panic!` outside tests, no `dbg!`, `todo!` or commented-out code
- [ ] No magic numbers or strings, no separator comments, no `#[allow(dead_code)]`
- [ ] No comments except `// SAFETY:` and the `# Errors`, `# Panics` and `# Safety` sections
- [ ] Commit message in English.
- [ ] No debug code or temp files

## Dependencies

- External dependencies are declared once in `[workspace.dependencies]` and referenced with
  `.workspace = true`.
- Do not add, remove or upgrade crates, or enable new features on existing ones, without asking.

## Environment

- Do not create or modify tool configuration in the repo (`.vscode/`, `.cursor/`, `.zed/`,
  `rustfmt.toml`, `clippy.toml`, `[lints]` tables, pre-commit hooks): the style configuration is
  personal. The lint set is passed on the command line through `just`. The only shared tooling
  files are `rust-toolchain.toml` (pinned nightly) and the `justfile`; change them only when asked.

## Scope and Git

- Keep changes minimal and focused on the task. Ask before large refactors, renaming public items,
  moving modules or changing crate structure.
- Do not commit, push, create branches or run destructive git commands (`reset --hard`,
  `push --force`, `clean`, `stash drop`) unless asked.