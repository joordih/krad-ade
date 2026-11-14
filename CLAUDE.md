# CLAUDE.md

Engineering guide for agents working on krad-ade, a desktop ADE (GPUI) to run many coding agents
at once, as chats or terminals, organised in sessions. `AGENTS.md` points here; this is the single source of these rules.

## Architecture

Virtual workspace: the root `Cargo.toml` has no `[package]`, only `[workspace]`,
`[workspace.package]`, `[workspace.dependencies]` and the dev profile overrides. One responsibility
per crate:

| Crate | Responsibility |
|---|---|
| `krad-domain` | Pure domain: ids, environments, groups, sessions and their panes, agents and harnesses, notes, settings, the review types every code host reads into, the git reads of a group (status, line counts, refs, worktrees) and the patches of a session's changes, the chat log (messages, their folding and per-message changes) and the asks of a chat, Kraddies with their automations, run history, memory and chat digests, variants, plan usage, the typed app events with their delivery and webhooks, the Agents API's requests, responses, handles and error codes (`agents_api`, re-exported by `krad-agents`), the harness catalog and slash commands, the ACP offers and the open-buffers seam, the origin of a hosted cloud session, and the persisted layout format. No GPUI or terminal types and no I/O. |
| `krad-pty` | Process spawning and PTY I/O over `portable-pty` (Unix PTY, Windows ConPTY) behind a platform-neutral API; platform shells in `unix.rs`, `windows.rs`, `wsl.rs`. |
| `krad-term` | Terminal emulation over libghostty-vt, linked from prebuilt static libraries. Bytes in, renderable snapshots out, key and mouse input encoded. |
| `krad-session` | Glue: owns a PTY + terminal pair, spawn/reader/writer threads, emits session events (`PumpMsg`) into any `PumpSink` (the runtime's pump queue). Re-exports the PTY and terminal items `krad-runtime` and `krad-ui` need, so neither depends on `krad-pty` or `krad-term` directly. |
| `krad-store` | The app's files on disk: config folder lookup, the layout split in two files (the runtime's state and the UI's view preferences) with their latest-wins writer thread and the one-time migration of the old single layout file, `settings.json`, the GitLab and GitHub logins, the Gateway key and the MCP server secrets (system keyring, config-folder fallback, all behind a `Secrets` handle whose in-memory backend keeps tests off the keyring), the images pasted into chats (`ChatImages`, rooted where the runtime is told), the folder of Gateway chat sessions, the transcripts of ACP chats, the event journal of each group, the webhook secrets, and the Kraddies with their automations, run histories, profiles and chat digests, including the migration of the old pets folder. Synchronous. |
| `krad-repo` | A Group's root folder: directory listing, the file index behind `@` mentions, text file reads, git status and worktrees through the `git` CLI. Synchronous; the runtime calls it from its job threads. |
| `krad-chat` | The live chat of each harness: Claude Code's stream-JSON process, Codex's app server and opencode's server, their asks, histories and model lists, the skills a message can call, and the Gateway's model list and the app's own agent engine that chats through it (agent loop, Anthropic Messages, OpenAI Responses and Chat Completions streams, its tools: files, search, shell, web fetch and search, questions, plans, checklists and subagents, an MCP client over stdio and HTTP, permissions and its session files). |
| `krad-gitlab` | GitLab's REST API v4: OAuth sign-in (Authorization Code with PKCE through a loopback redirect, Device Authorization Grant as fallback) and token refresh, merge requests, commits, changes, discussions, notes, approvals, merge, pipelines and jobs, over `ureq`. Synchronous; the UI calls it from the background executor. |
| `krad-github` | GitHub's REST API, with GraphQL for review threads: device-flow sign-in, pull requests, commits, changed files, conversation comments, reviews, review threads, approvals, merge, Actions runs and jobs, read into the review types of `krad-domain`, over `ureq`. Synchronous; the UI calls it from the background executor. |
| `krad-update` | App updates from the app's own GitHub releases: the signed `latest.json` manifest (minisign, public key in `packaging/update/minisign.pub`), the verified download, and replacing the installation per platform (`installation/macos.rs`, `linux.rs`, `windows.rs`) after the install marker shows it is a release build, unpacking with the system `tar`. Synchronous; the UI calls it from the background executor. |
| `krad-host` | The machine an environment lives on: this computer or an SSH host reached through the system `ssh` (a ControlMaster per host on Unix; on Windows one long-lived session per host running the host CLI's `serve`, a multiplexer of commands, terminals (a pseudo-terminal on the host through `portable-pty`), the host's `sftp-server` and port forwards, signed in through `SSH_ASKPASS`; a hand-written SFTP v3 client for files, commands built as POSIX scripts, local and reverse port forwards, files written with a mode, the Linux host check), and the client side of a cloud runner: its folder layout (sockets, pairing files, the offline event queue), the process wire and its verbs (`cloud-attach`, `cloud-stop`, `cloud-resize`, `cloud-status`, `cloud-connect`, the stdio bridge to the runner's runtime socket) and the installation of its service over SSH. Synchronous; the other crates take a `HostLink` instead of touching `std::fs` or spawning a process for a group's files. |
| `krad-lsp` | Language server client over stdio: starting a server here or on the environment's host, `file://` URIs, requests, notifications and diagnostics. |
| `krad-extension-id` | The extension id and its rule (one safe path segment), with no I/O; published to crates.io and shared by `krad-domain` and `krad-extension-api`. |
| `krad-extension-api` | The protocol between the app and extension programs, with no I/O: the `extension.toml` manifest, ids and the API version, the JSON-RPC requests and notifications in both directions, the extension view model (building blocks, ready-made trees, tables, forms and streams, keyed patches, events) and the JSON Schema published in `sdk/schema/`. |
| `krad-extensions` | Language, toolchain and server runtime manifests and the package registry: locating, installing and launching language servers and their server runtimes, and finding a project's toolchains (data-driven locators, project hints, ranking, cache) and handing the chosen one to its server, here or on an SSH host through `krad-host`. |
| `krad-agents` | Agent coordination bus, after Orca's orchestration: the loopback endpoint the app serves, its line-JSON protocol versioned as the agents API (version handshake, stable error codes; the request and response types live in `krad-domain`'s `agents_api` and are re-exported here, the token and envelope stay here), the mailbox, and the `krad` CLI (alias `krad-ade-agents`) agents run from their shell, which on a host also serves the multiplexed session and, on this computer, carries its commands and sign-in prompts, and detects Python environments with Microsoft's `pet` (feature `python-toolchains`, enabled by `krad-app`). |
| `krad-protocol` | The protocol between the runtime and its clients, with no I/O but its codec: the protocol version and the handshake (`Hello` with an optional pairing, `Welcome` with the runtime's build and run, the granted permissions and the connection's first request id), request ids, `Seq`, `Subscribe { since }`, commands wrapped per area, replies with stable codes, events, snapshots per scope, the pairing commands and answers and the `ask_snapshot`/`snapshot`, `pairing`/`paired` and `refused` messages a remote connection uses (newer messages, areas, verbs, events and codes read as `Unknown`), the hosted-session metadata of a runner's snapshot (`HostedSummary`, built by one pure `hosted_summaries` shared by the runner and its clients), and the length-prefixed frames over `io::Read`/`io::Write`, JSON for messages and raw bytes for terminal data; its examples generate and test `sdk/schema/krad-runtime-protocol.json`. |
| `krad-runtime` | The headless runtime that will own the live state while the UI becomes its client (strangler migration, ADR-0051 to ADR-0053): one `krad-runtime` thread that drains one bounded `Input` channel (block on the first, drain the rest, apply the batch, then flush), its own timer tick, monotonic `Seq`, and an event bus that keeps a ring of recent events, so a client resubscribing `since` a `Seq` gets only what it missed (or a snapshot when that is too old), and whose slow subscribers fall back to a fresh snapshot; it answers the handshake with its build and run; a cloneable `RuntimeHandle` sends commands, subscribes and asks for snapshots, and the thread stops when every handle is dropped. It serves the agent bus: the listener feeds its `Input`, it answers the requests on its own state (mailbox and `check`, status lines, held asks and their first-wins answers, `list` from the directory the app reports) and hands the rest to the UI as delegated requests whose `BusReply` comes back through it. It publishes the app events the UI and its own sources raise, including the session opened and closed events it finds by comparing each environment's sessions after a batch (the event journal, the webhook sender with its retries, secrets lookup and test deliveries, and each published record as an event) owns the attention queue and its history, raised, seen and dismissed by commands and announced as events, and owns the harness health checks, the plan usage readings with their schedule, shared budget and own timer, and the harness model lists per host, the Gateway check (a command whose outcome is an event) with its key, which never leaves it, the opencode server of each host (started or attached, its models and catalog watch, shared read-only with the UI), the live chats of Claude Code, Codex, opencode, ACP agents (their offers, early choices, sign-in, secrets and transcripts) and the Gateway, whose launches it resolves itself at every start from a brief the UI sends (runner, system prompt with the lead or member hint, bus variables, offered model, Gateway engine) along with their MCP servers (the project file read through the node's link, the trust question as chat state answered once, secrets read in a job, the code tools server), and the bus requests' lead resolution and refusal and closed workers' `wait` and `read`, and detects the agent CLIs on this computer and in WSL, and owns the environments' persisted data (the only writer of the state file and of every node, session, group and the archive, through typed session, group and node edits the UI applies optimistically and replaces with the runtime's state once they are answered; per-item environment changes out and the client's title, host and variants runs in, the spending tally and budget crossings, and each terminal's meta, its folder and saved scrollback, which only it writes), owns the settings (read at start, the only writer of `settings.json`, `settings_changed` to every client), and runs the terminals: each PTY and its emulator live on the runtime thread; an `open` command with a launch intent is resolved by the runtime itself (shell, agent launch line from the settings, lead role, bus variables, opencode attach), held until the environment's host and its remote bus are ready, and the missing sessions of an environment are reopened and its host's agent CLIs detected when the host connects, while the UI still hands it resolved launches for sign-in and command terminals; it places them here, in WSL, on an SSH host or through an opener, spawns it, takes its output through its own pump queue, classifies agent terminals, and publishes each terminal as a read-only `TerminalView` whose latest frame it renders only when a client painted the previous one, with `terminal_changed`, `terminal_ended` and `terminal_classified` events and input, resize, scroll and close as commands, and opens a terminal beside a chat in that chat's session (`open_beside`, answered with the terminal it opened); it starts the worker of every `spawn` (a lead's in its session, any other caller's in the caller's group without a lead), follows, reopens and closes a lead's chat workers (from the bus and from `AgentCommand`s), answers every `wait` and answers `read`, `send` and `list --all` on its own chats and terminals and `sessions`, `create-session` and `close-session` on its environments; a new subscriber gets its snapshot when it subscribes and every event after it; it answers terminal agents' hooks (marks the terminal hooked, keeps the node's resume id and announces each hook as an event) and types into terminals (prompts after a start, submits with confirmed Enter retries, notices and key steps); runs Claude Code chats (the UI hands it a resolved chat start; it starts the live process, folds its events into the chat's messages, holds its asks with first-wins answers and raises them as attention, loads the history to resume and emits one event per changed message); it owns each environment's host lifecycle (one `HostLink` per host, shared read-only with the UI for its file I/O, the connect, sign-in, retry and drop state machine run as jobs, the SSH sign-in terminal it opens and watches itself, the sign-in prompts as events answered by commands, the remote agent bus prepared after a connect, and machine connections through a channel the UI hands it from the extension); serves the code hosts (sign-in and its state, the review's reads and writes, "Fix with agent", the CI poller) with the tokens kept in the runtime and the keyring, and owns the cloud runners' connections (connect, install and pairing, their state, removal and queued stops, each runner's agent bus with its liveness watch, the catch-up of the events a runner queued while away (raising attention for the asks it replays) and the offline webhooks synced to it, and the line to each ready runner's own runtime: `ssh … cloud-connect` with the pairing in its `Hello`, handed out in-process by `connect_runner`), keeping every runner's link and pairing to itself, and runs the cloud sessions' chats and terminals on their runner (the process id, liveness check and saved journal offset of each node, attach and reattach from that offset with the trimmed-journal retry, the runner's opencode server, deferred launches while a runner is away and queued stops, the chats a new cloud session carries), hands a new cloud session over to the runner's runtime (`cloud/host` over a one-shot line) and prepares it over the process wire instead when that runtime does not welcome it, and, hosted on a runner, drives that runner's own daemon as its own runner over the local link (`OwnRunner`): it creates cloud sessions there from `cloud/host` (clone, worktree, branch, patch, chats) and reopens their chats itself at start; every secret it reads or writes goes through the `Secrets` handle it is started with; blocking work (probes, usage reads, listings) runs on one short-lived job thread each whose result comes back through `Input`, and it runs the groups' git: it polls each environment's group roots on its own paced schedule and publishes their reads, carries out git commands (fetch, pull, push, stage, commit, branches, tags, discards), makes and removes worktrees (also for a lead's workers and `close --remove-worktree`), keeps and reads each session's changes and reverts them, answers git's askpass through its sign-in prompts and sweeps merged worktrees; it owns the Kraddies and their automations (the only writer of their files, run histories, keepsakes, compass, activity and chat digests; the scheduler, catch-up runs, app and code-host triggers and rounds check-ins on its own injectable `Clock`; runs as worker chats, in a fresh worktree when asked; digests of the chats it closes or that went quiet; the memory preamble of every chat it starts; Kraddie chat sessions; the Kraddie, automation and memory bus requests and the memory gate) behind `KraddieCommand`s; it resolves ACP launches and keeps the ACP adapters the app installs, reading unsaved editor buffers through a seam the UI offers per environment; it runs variants (their worktrees, chats, progress, diff summaries, tests and the first-wins pick) behind `VariantCommand`s and the variant bus requests; it holds the code-host client over `krad-github` and `krad-gitlab` and watches the code hosts for CI and merge request events; it saves the images pasted into chats from their bytes and forgets them with their chat, and reports what failed as `problem` events (scope, code, message) the UI shows as it sees fit; it lists the ACP agents' models and offers, reads chat histories for search (`chat/history`), opens ACP sign-in terminals with the agent's secrets, follows the opencode sessions of attached terminals, checks that a file stays inside its group folder and removes a lead's settled worktrees; on a runner it reports hosted sessions with their origin and metadata in its snapshot, raises a `problem` for a hosted session that could not be prepared, and keeps a short replay of its own terminals for watchers (trimmed to start at an escape so no sequence is cut in half), and reads a kept chat image back as bytes for a remote client (`chat/read_image`, only inside its image folder, bounded size); the UI opens terminal views, runner lines and takes bus delegations through in-process calls on the handle. |
| `krad-client` | The client side of the runtime protocol: the `RuntimeClient` trait (`hello`, `send`/`post` of a `Command` for its `Reply`, `subscribe(since)` to a stream of `Feed`s, `snapshot(scope)`, `watch_terminal` refused `unsupported` but by `Runner`), the `Cursor` that resumes a client from its last `Seq` only within the same runtime run, and `InProcess`, its implementation over a `RuntimeHandle`, which also hands out that handle for the in-process-only extensions (terminal views and actions, host links and machine channels, open editor buffers, the bus delegation channel and its access, and withdrawing the discovery file at window close), and `Runner`, its implementation over any byte stream to a runner's runtime (the Unix socket, or `cloud-connect` through SSH): the paired `Hello`, the connection's own request id range from `Welcome`, replies, snapshots and pairing answers matched by request id, the feed, terminal byte feeds (`watch_terminal`) and a welcome timeout. |
| `krad-runner` | The cloud runner daemon, and the only crate that knows it is a daemon: the detached processes it keeps (journal, PTY and piped processes, adoption after a restart, the process wire's server), the pairings (hashed tokens with device, date, last use and `read`/`drive`/`answer`/`admin` permissions, one-use pairing codes) and the runner's verbs (`cloud-runner`, `cloud-pair`, `cloud-unpair`). With its `runtime` feature (default, off in `krad-app` unless `runner-runtime`) it also starts a `krad-runtime` with the runner profile (no window, one local environment, its files and pasted chat images under the runner folder) and serves `krad-protocol` on `runtime.sock` through a `Listener`, answering the pairing commands itself and checking each command's permission, and pairs that runtime with its own daemon at every start so it runs cloud sessions itself; `serve_runner(root, secrets)` takes the `Secrets` it hosts the runtime with (`cloud-runner` passes the system keyring, tests an in-memory one). |
| `krad-ui` | All GPUI views, elements, actions, the per-platform keymap (`keymap.rs`: Cmd on macOS, Ctrl+Shift on Windows/Linux so shells keep their control keys), and the light/dark theme. |
| `krad-app` | Binaries: `krad-ade` (bootstrap, wiring, starting the runtime before the window, window creation; kept thin; needs the default `desktop` feature) and the console `krad` CLI with its `krad-ade-agents` alias, both of which hand the runner's verbs to `krad-runner` and everything else to `krad-agents`; the alias also builds without `desktop` as a static musl binary for SSH hosts and cloud runners (`just agents-linux`, for x86_64 and aarch64), with the `runner-runtime` feature, so its runner hosts the runtime: Zig is the cross C toolchain (`packaging/zig/` wraps it for `ring`) and builds `libghostty-vt` for musl from the pinned Ghostty commit (`just ghostty-libs-musl`, kept under `target/`). |

**Dependency rules (mandatory):**

- `krad-app -> krad-ui -> {krad-session -> {krad-pty, krad-term}, krad-store, krad-gitlab, krad-github, krad-update, krad-lsp, krad-extensions}`,
  `krad-app -> krad-agents`, `krad-app -> krad-store`, `krad-app -> krad-runtime`, `krad-app -> krad-client`, `krad-app -> krad-runner`, `krad-runner -> {krad-domain, krad-host, krad-protocol, krad-runtime, krad-store}` (all but `krad-domain` and `krad-host` only with its `runtime` feature), `krad-ui -> {krad-runtime, krad-protocol, krad-client}`, `krad-client -> {krad-protocol, krad-runtime}`, `krad-runtime -> {krad-protocol, krad-agents, krad-store, krad-chat, krad-host, krad-session, krad-repo, krad-github, krad-gitlab}` and `krad-extensions -> krad-extension-api`, which `krad-extensions` re-exports to `krad-ui`; `krad-extension-id` depends on no workspace crate; `krad-domain` and `krad-extension-api` depend on it and re-export the id, and `krad-extension-api` depends on no other workspace crate. `krad-extension-id`, `krad-extension-api` and `sdk/rust` (`krad-extension-sdk`) are published to crates.io, so their workspace dependency entries carry a version. `krad-repo`, `krad-chat`, `krad-lsp`, `krad-extensions` and `krad-agents` may depend on `krad-host`, which depends only on `krad-domain`. Any crate may depend on `krad-domain`. No other edges, no cycles.
- `krad-domain` depends on no workspace crate but `krad-extension-id` and on none of `gpui`, `libghostty-vt` or `tokio`.
- `krad-term` must not depend on `krad-pty`; it is tested by feeding raw bytes.
- `krad-protocol` depends on no workspace crate but `krad-domain`, with no network I/O and no
  threads. Its types are defined once there and re-exported, never mirrored in `krad-ui` or the SDKs.
- `krad-runtime` must not depend on `gpui`, `krad-ui` or `tokio`. As subsystems move into it, it may
  depend on `krad-protocol`, `krad-domain`, `krad-host`, `krad-chat`, `krad-repo`, `krad-agents`,
  `krad-session`, `krad-store`, `krad-github` and `krad-gitlab`, and it becomes the only crate that
  writes the layout and the sessions' state through `krad-store`.
- `gpui` (published as `gpui-pre`, pulled in by `gpui-kit`) is only allowed in `krad-ui` and
  `krad-app`.
- `krad-runner` is the only crate that knows it is a daemon: the runtime it hosts is the same
  `krad-runtime` the desktop runs, started with a profile, and no other crate depends on
  `krad-runner` but `krad-app` (and `krad-runtime`'s tests, for the fake runner).
- `krad-ui` must not depend on `krad-chat`, `krad-repo`, `krad-host` or `krad-agents` (`just
  ui-coupling` stays at 0). What it needs from them is a runtime command, or a pure type moved down
  to `krad-domain` or `krad-protocol`; the in-process seams that must stay use types from crates it
  may depend on (host links through `krad_extensions::host`).
- `krad-ui` speaks to the runtime through `krad-client`: commands, events and snapshots go through
  `RuntimeClient` (`self.runtime.client`), and the read copy of the runtime's state is the shell's
  `ViewModel`, folded from those feeds (settings, git, Kraddies with their minds and digest books, and
  each environment's optimistic copy, which the shell holds as an `Rc` handle and edits only through
  typed edits and commands that update the `ViewModel` first; the shell keeps view preferences only): the local runtime's, and one `krad_client::Runner` per ready
  cloud runner whose state (environments, chats, asks, model lists, hosted sessions) is kept apart per `RuntimeId` (`ViewModel.remotes`); a runner whose
  runtime does not welcome the desktop stays on the process wire and reads "Update pending".
  A chat is addressed by a `ChatPlace` (`RuntimeId` and environment) and its `TerminalId`, so the
  one chat surface shows local chats and runner-hosted ones alike and sends each chat's commands
  through its own runtime's client; a runner's terminals are watched through
  `RuntimeClient::watch_terminal` and painted from a local emulator.
  Detected agents, the Gateway's state and the code hosts' logins are read from the `ViewModel` too, never kept in the shell.
  Settings, the runtime's environments, the groups' git reads and the Kraddies are owned by the
  `ViewModel` and read through the `Rc` handles `RuntimeLink::reread` re-points after each batch
  (`self.runtime.settings`, `.environments`, `.git`, `.kraddies`); settings change only through
  `edit_settings` (or `preview_settings` for a live preview), never by writing a shell copy.
  The editor is the one exception the plan keeps UI-side: language servers, extensions, toolchains and the unsaved
  buffers of file and note editors live with the window that edits them. The app's own update
  (`krad-update`) is the second: it replaces the process that owns the window, and the runtime may
  not depend on `krad-update`.
  `RuntimeHandle` (`self.runtime.handle`) is only for what can
  never cross a wire: terminal views and actions, host links and machine channels, runner lines,
  open editor buffers, the bus delegation channel and its access, and withdrawing the discovery file. Pasted chat images go to the runtime as bytes (`chat/attach_image`),
  never as client paths, and a runner-hosted chat's images come back as bytes (`chat/read_image`); the backdrop images are view preferences, not settings.

Inside `krad-ui`, the root view lives in `environment_shell.rs` and each concern of it (sessions
and their panes, chat surfaces, terminal bodies and actions, sidebar, groups, file tree, file
tabs, notes, review, side panel, utilities panel, settings, gateway, app updates, app events, agent bus, runtime view, agent hooks, hosts, context menus,
Kraddies and their chats and memory, automations, variants, usage bar, attention inbox, session changes,
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

- Use `tracing`, never `println!`/`eprintln!`, outside tests and the CLI's own output. `krad-app`
  writes it to a daily rotated file in the config folder's `logs/`, filtered by `KRAD_ADE_LOG`.
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