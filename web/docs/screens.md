# Screens

Every desktop screen to replicate, in build order: the first ones make the web usable remotely, the
later ones complete the replica. Source folders are under
`../../crates/krad-ui/src/environment_shell/` unless written otherwise. Tick a box only when the
screen passes step 4 of the loop in `../AGENTS.md`. `[~]` marks a screen that exists in a first
version: finish what its line says is missing.

## 1. Shell

- [~] **Pairing and connection.** Pair with a code, show connection state (connecting, waiting to
  retry, refused, open), forget this browser. Done: code entry, `#pair=`, connecting and
  reconnecting lines (`src/ui/Pairing.tsx`, `App.tsx`). Missing: forget this browser. Desktop counterpart: Settings → Cloud runners pairing.
  `src/runtime/http.ts`, `client.onStatus`.
- [ ] **Window frame, backdrop, top bar.** `../window_frame.rs`, `backdrop.rs`, `top_bar.rs`,
  `../theme.rs` (backdrop gradient, surface levels, shadows: `--krad-shadow-*`).
- [~] **Sidebar.** Environments, groups, sessions, agent status dots, attention badges. `sidebar/`,
  `sessions_view.rs`, `attention/`. Areas: `environments`, `session`, `group`, `attention`. Done:
  environment switch, groups with branch, sessions with state, cloud sessions merged in
  (`src/ui/Sidebar.tsx`). Missing: attention badges, new, rename, close session, kraddie sessions.
- [ ] **Panes and tabs.** Session panes, tab strip, splitting, focus, drag to reorder. `session_pane/`,
  `layout.rs`, `surface.rs`, `crates/krad-domain/src/persisted_layout/` (from the repo root) for its types. Tabs are runtime state (`node` area); pane
  arrangement is per browser (keep it in `localStorage`).
- [ ] **Command palette.** `palette/`, `../components`, key bindings in `../keymap.rs`.
- [ ] **Toasts and dialogs.** `toasts/`, `dialog_parts/`, `name_dialogs/`, `context_menu/`.

## 2. Chat and terminal

- [~] **Chat transcript.** Messages, tool calls, diffs, thinking, images, bookmarks, checkpoints,
  earlier pages. `surface/` (largest folder), `chat/`, `chat_search/`. Area: `chat`. Done: messages,
  markdown, thinking, tool rows, earlier pages, follow scroll (`src/ui/chat/`). Missing: tool output
  and diffs, images, checklists, bookmarks, checkpoints and rewind, search, opening a new chat.
- [~] **Composer.** Text, mentions, attachments (`/upload` then the path), queueing, interrupt, model,
  effort, permission mode, slash commands. `surface/composer*`, `surface/mentions*`, `model_picker/`.
  Done: send, queue while working, interrupt, shows model and permission mode. Missing: changing
  model, effort and permission mode, mentions, attachments, slash commands, the queued list.
- [~] **Asks, questions and plans.** `ask/`, `surface/answer*`, `surface/choice.rs`. Areas: `chat`,
  `bus` (`answer_ask`). Done: permission, questions, plan approval, ACP options
  (`src/ui/chat/AskCard.tsx`). Missing: deny with a message, the desktop's exact wording.
- [ ] **Workers.** `workers/`, `agent_bus/` (lead and members).
- [ ] **Terminal.** Emulator fed by `watchTerminal`, keys and resize back, search, selection, links.
  `terminal_body*`, `terminal_actions.rs`, `live_terminal.rs`, `input.rs`. Area: `terminal`.

## 3. Git and review

- [ ] **Changes tab and session changes.** `changes_tab/`, `session_changes/`, `git_actions/`.
  Areas: `git`, `changes`, `repo`.
- [ ] **Diffs and history.** `review_view/`, `code_viewer/`; use `@pierre/diffs` styled with tokens
  and highlighted with the Shiki theme from `AGENTS.md`.
- [ ] **Code review (GitHub, GitLab).** `review_view/`, `cloud/` sign-in parts. Areas: `review`,
  `code_host`.

## 4. Files and editor

- [ ] **File tree.** `file_tree/`, live through `file/watch` (`folder_changed` events), file icons from
  `../../assets/file-icons`.
- [ ] **File viewer.** Read-only, Shiki-highlighted, with line numbers and the file's diagnostics.
  `code_viewer/`, `file_tab/` (look only). Area: `file` (`read`). No editing: the owner chose
  viewing over editing for the web.
- [ ] **Problems.** `file_tab/problem_list.rs`, `activity/problems.rs` from `diagnostics` events.
- [ ] **Search and replace.** `side_panel/search*`. Area: `file` (`search`, `replace`).

## 5. Everything else

- [x] **Notifications.** Installable app (`public/manifest.webmanifest`, `public/sw.js`) and Web Push
  when an agent waits or finishes (`src/runtime/push.ts`, bell in the sidebar). The runtime sends
  them; on iPhone they need the app added to the home screen.

- [ ] **Notes.** `notes_view/` (vault tree, editor, graph, inspector). Area: `file` (vault verbs).
- [ ] **Kraddies and automations.** `kraddies/`, `automations/`, `kraddie_memory/`. Area: `kraddie`.
- [ ] **Variants.** `variants/`. Area: `variant`.
- [ ] **Cloud sessions and runners.** `cloud/`. Areas: `cloud`, `runner`.
- [ ] **Usage and harness health.** `usage_bar/`, `harness_health/`. Areas: `usage`, `health`.
- [ ] **Archive.** `archive/`.
- [ ] **Settings.** `settings_view/` (largest screen): appearance and theme tuning (reload
  `/theme.css` after saving), harnesses, models, MCP, ACP, webhooks, code hosts, runners, extensions,
  skills. Area: `settings` (needs `admin`).

Not reachable from a browser yet, because the runtime does not run them: extension programs and
their machine views, servers that need a toolchain handoff (Python, Java), and terminals the desktop
launches itself. Show them as unavailable with the desktop's wording.
