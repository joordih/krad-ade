# Screens

Every desktop screen to replicate, in build order: the first ones make the web usable remotely, the
later ones complete the replica. Source folders are under
`../../crates/krad-ui/src/environment_shell/` unless written otherwise. Tick a box only when the
screen passes step 4 of the loop in `../AGENTS.md`. `[~]` marks a screen that exists in a first
version: finish what its line says is missing.

## 1. Shell

- [~] **Pairing and connection.** Pair with a code, show connection state (connecting, waiting to
  retry, refused, open), forget this browser. Done: code entry, `#pair=`, connecting and
  reconnecting lines (`src/ui/pairing/Pairing.tsx`, `src/ui/App.tsx`), forget this browser in the sidebar's small
  "This browser" menu (`forgetPairing`). Missing: the desktop's pairing screen states and wording.
  Desktop counterpart: Settings → Cloud runners pairing. `src/runtime/http.ts`, `client.onStatus`.
- [ ] **Window frame, backdrop, top bar.** `../window_frame.rs`, `backdrop.rs`, `top_bar.rs`,
  `../theme.rs` (backdrop gradient, surface levels, shadows: `--krad-shadow-*`).
- [~] **Sidebar.** Environments, groups, sessions, agent status dots, attention badges. `sidebar/`,
  `sessions_view.rs`, `attention/`. Areas: `environments`, `session`, `group`, `attention`. Done:
  environment switch, groups with branch, sessions with state, cloud sessions merged in
  (`src/ui/sidebar/`), new session in a group (with its first chat), rename and close a session,
  mute a session, attention badges and a jump button that walks the waiting queue, Kraddie sessions
  under their Kraddie, session menus by right click, long press or the phone's "..." button.
  Missing: the attention inbox with its history filter, session icons and colours, sidebar density.
- [~] **Panes and tabs.** Session panes, tab strip, splitting, focus, drag to reorder. `session_pane/`,
  `layout.rs`, `surface.rs`, `crates/krad-domain/src/persisted_layout/` (from the repo root) for its types. Tabs are runtime state (`node` area); pane
  arrangement is per browser (keep it in `localStorage`). Done: the tab strip with focus, a new chat
  (harness-agnostic: its model picks the harness), a new terminal with a chosen harness, close and rename a tab, and
  tab menus by right click, long press or the phone's "..." button (`src/ui/session/`).
  Missing: splitting, drag to reorder and the per-browser pane arrangement.
- [ ] **Command palette.** `palette/`, `../components`, key bindings in `../keymap.rs`.
- [ ] **Toasts and dialogs.** `toasts/`, `dialog_parts/`, `name_dialogs/`, `context_menu/`.

## 2. Chat and terminal

- [~] **Chat transcript.** Messages, tool calls, diffs, thinking, images, bookmarks, checkpoints,
  earlier pages. `surface/` (largest folder), `chat/`, `chat_search/`. Area: `chat`. Done: messages,
  markdown, thinking, tool rows, earlier pages, follow scroll, and opening or resuming a chat that
  the runtime has not started yet from its tab (`src/ui/chat/`). Missing: tool output
  and diffs, images, checklists, bookmarks, checkpoints and rewind, search.
- [~] **Composer.** Text, mentions, attachments (`/upload` then the path), queueing, interrupt, model,
  effort, permission mode, slash commands. `surface/composer*`, `surface/mentions*`, `model_picker/`.
  Done: the desktop's classic composer (`src/ui/chat/composer/`): field with send/stop, permission
  mode, run-on chooser, model menu (harness rail with every harness found here while the chat is
  fresh, so picking another harness's model moves the chat to it as the desktop does: close, node
  harness, open, set model; search, sections, badges, favorites and default marks for admins), effort with its meter, context ring, `@` file mentions and `/` skills from the
  chat's commands, attachments uploaded into the group folder, queued list with cancel; popovers on
  wide screens, bottom sheets on phones. Missing: the effort slider popover (a list here), extended
  thinking, the orchestrator toggle, ACP mode and option controls, image attachments, the Tray
  composer design.
- [~] **Asks, questions and plans.** `ask/`, `surface/answer*`, `surface/choice.rs`. Areas: `chat`,
  `bus` (`answer_ask`). Done: permission, questions, plan approval, ACP options, denying with a
  message and plan feedback (`src/ui/chat/AskCard.tsx`). Missing: the desktop's exact wording
  everywhere else.
- [ ] **Workers.** `workers/`, `agent_bus/` (lead and members).
- [~] **Terminal.** Emulator fed by `watchTerminal`, keys and resize back, search, selection, links.
  `terminal_body*`, `terminal_actions.rs`, `live_terminal.rs`, `input.rs`. Area: `terminal`. Done:
  ghostty-web emulator fed by `watchTerminal`, keys and grid size back, the runtime emulator's own
  query replies discarded, theme from `--krad-ansi-0..15`, and an extra-keys row on phones (Esc,
  Tab, sticky Ctrl, arrows, Ctrl+C) (`src/ui/terminal/`). Missing: search, links and the desktop's
  terminal actions.

## 3. Git and review

- [ ] **Changes tab and session changes.** `changes_tab/`, `session_changes/`, `git_actions/`.
  Areas: `git`, `changes`, `repo`.
- [ ] **Diffs and history.** `review_view/`, `code_viewer/`; use `@pierre/diffs` styled with tokens
  and highlighted with the Shiki theme from `AGENTS.md`.
- [ ] **Code review (GitHub, GitLab).** `review_view/`, `cloud/` sign-in parts. Areas: `review`,
  `code_host`.

## 4. Files and editor

- [~] **File tree.** `file_tree/`, live through `file/watch` (`folder_changed` events), file icons from
  `../../assets/file-icons`. Done: the session group's folder, expand and collapse with a lazy
  `file/list`, `file/watch` and `file/unwatch` for the root and every open folder, live
  `folder_changed` updates, recoloured file and folder icons, error and warning underlines from
  `diagnostics`, git change colours with their M/A/D/R/U/! letters and the dot on folders holding
  changes (from the group's `git_read`), the root folder row with its chevron and the reload and
  collapse-all pill, a click opening a preview tab and a double click a pinned one, in a side panel
  right of the session with the Explorer and Changes sections (Changes: staged and unstaged lists
  with line counts, the section kept per browser, the header's +/− totals), toggled by the strip's
  right panel button or Cmd+Alt+B (Ctrl+Alt+B off macOS), its open state kept per browser; a bottom
  sheet on phones (`src/ui/files/FilesPanel.tsx`, `src/ui/files/ChangesList.tsx`,
  `src/state/files.ts`, `src/state/git.ts`). Missing: the Changes toolbar, commit box, history and
  staging, resizing the panel, drag and drop, upload, create, rename, move and delete,
  package-folder folding, the parent-folder menu and the search and new-entry header buttons.
- [~] **File viewer.** Read-only, Shiki-highlighted, with line numbers and the file's diagnostics.
  `code_viewer/`, `file_tab/` (look only). Area: `file` (`read`). Done: reading through `file/read`,
  line numbers, the language picked from the file name, grammars loaded lazily, a Shiki theme built
  at runtime from the 14 `--krad-syntax-*` properties (comments italic, headings bold, embedded code
  in the text colour), rebuilt when `/theme.css` reloads, per-line diagnostic markers and a problems
  list (`src/ui/files/FileViewer.tsx`, `src/ui/highlight/`). Files open as tabs in the session's
  strip, after its runtime tabs, with the desktop's preview semantics (a preview tab in italics is
  replaced by the next preview, a double click or opening it pinned keeps it, an open file is
  focused instead of opened twice, Pin Tab and Close Tab in its menu), and the selected tab fills
  the main area. These file tabs are kept per session in the browser (`src/state/fileTabs.ts`),
  not as runtime `File` nodes; a file the runtime already has open as a `File` node (opened on the
  desktop) is shown and focused through that node, and pinning it sends `node/pin_file`. Missing:
  editing is out by the owner's decision; no minimap, problem lens marks inside tokens, images or
  binaries, the changes and commit views of a file tab.
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
