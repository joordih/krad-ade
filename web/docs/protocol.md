# Protocol

The runtime speaks `krad-protocol` (ADR-0052 in `../../docs/adr/`). Its Rust source is
`../../crates/krad-protocol/src/`; its TypeScript shape is `../src/protocol/`. A browser reaches it
through web access (ADR-0056, `../../crates/krad-web-listener`).

## Transport

- One WebSocket per runtime at `/runtime`, same origin as the page. Each WebSocket message is one
  frame: a kind byte then its body. Kind `1` is a JSON `Message`; kind `2` is terminal output: an
  8-byte big-endian terminal id, then raw PTY bytes. `src/runtime/frames.ts` does both.
- The socket only opens from the page's own origin; the pairing token rides in an `HttpOnly` cookie
  and the server puts it into the `hello`. Script never sees the token.

## Session

1. Send `hello` (`protocol: 1`, `build`). The answer is `welcome` (with `run`, `permissions` and
   `first_request`) or `refused` (not paired, other major version).
2. Number requests from `welcome.first_request`, incrementing; the runtime refuses ids outside the
   connection's range.
3. Send `subscribe`. The first `feed` is a full snapshot (`feed: "snapshot"`), then events
   (`feed: "event"`), each with a rising `seq`.
4. After a reconnect, if `welcome.run` equals the previous run, send `subscribe { since: lastSeq }`
   to receive only the missed events (or a fresh snapshot when too many were missed). A different
   `run` means the runtime restarted: subscribe from scratch. `RuntimeClient` does all of this.

## Permissions

`read` (follow, snapshot, watch terminals), `answer` (asks, questions, plans), `drive` (everything
that changes state, uploads), `admin` (settings, runners, pairing codes). `welcome.permissions` says
what this browser holds; hide or disable what it lacks rather than letting the runtime refuse.

## Commands

A `command` names an `area` and usually a `verb` (see `Command.ts` and each `*Command.ts`). The
runtime replies at once with `reply`: `{ outcome: "accepted" }` or
`{ outcome: "refused", code, message }`. What the command did arrives later as events everyone sees.

Some commands produce a result for the caller only: `file`, `repo`, `language`, and several `chat`
reads. Their result arrives as an `answered` event carrying the request id. Use
`client.ask(command)`, which resolves with that `Answer` (or rejects with `RuntimeError(code)`).

Areas worth knowing first:

| Area | What it covers |
| --- | --- |
| `environments`, `session`, `group`, `node` | the sidebar: environments, groups, sessions, their tabs (nodes) |
| `chat` | chat pages, sending, queueing, models, effort, asks, questions, plans, images |
| `terminal` | open, write, resize, scroll, close, `watch` for output bytes |
| `git`, `changes`, `repo` | status, staging, commits, branches, diffs, history, session changes |
| `review`, `code_host` | GitHub and GitLab review, sign-in |
| `file` | read, write, list, search, replace, notes vault, `put` (bytes), `watch` (file tree) |
| `editor`, `language` | runtime buffers and their language server (see `editor-sync.md`) |
| `kraddie`, `variant`, `cloud`, `runner` | Kraddies and automations, variants, cloud sessions and runners |
| `settings`, `attention`, `usage`, `models`, `health` | the rest of the shell |

## Events and the snapshot

`Snapshot.ts` lists everything the first feed carries; `EventKind.ts` lists every event. A reducer
applies events to the snapshot by `type`. Unknown event types and unknown enum values must be
ignored, never treated as errors: the protocol only grows by adding (`Unknown` variants exist on the
Rust side for this).

## Terminals

- Terminal output is raw bytes for a terminal emulator. `client.watchTerminal(agentRef, onBytes)`
  sends `terminal/watch`; the first bytes are a replay of up to 64 KiB, then live output. Terminal
  frames carry only the terminal id, so keep one watch per id.
- Feed the bytes to an emulator (ghostty-web first, xterm.js if it falls short), encode keys in the
  emulator and send them with `terminal/write` (`by: "user"`), report the grid with
  `terminal/resize` (`GridSize`: cols, rows, cell pixel size). Selection, search and scrollback stay
  in the browser. Discard the emulator's own replies to terminal queries: the runtime's emulator
  already answered them.

## Bus delegations

Agents call the agents bus; the runtime answers most requests itself and delegates the rest (open a
file, some Kraddie and extension requests) to one client. `client.onDelegated(listener)` makes this
browser that client and `client.respond(delegation, response)` answers. Only one client holds the
delegations at a time, and the desktop app takes them too, so offer it as an explicit toggle
("answer agent requests here") and answer every request it receives, with `Response` errors when the
browser cannot do what was asked.

## HTTP paths

`src/runtime/http.ts` wraps them:

| Path | Use |
| --- | --- |
| `POST /pairing` | redeem a pairing code into the cookie (`redeemPairingCode`) |
| `DELETE /pairing` | forget this browser's cookie (`forgetPairing`) |
| `POST /upload?environment=&dir=&name=` | raw file body into a folder, needs `drive` (`upload`); for attachments, then pass the returned path |
| `GET /theme.css` | the `--krad-*` properties for the runtime's theme and tuning |

## Gotchas

- 64-bit ids are plain `number`s in JSON; they stay below 2^53.
- A `TextEdit` in `editor` uses flat UTF-16 offsets; `language` positions use `{ line, character }`
  in UTF-16. JavaScript strings are UTF-16, so CodeMirror positions map directly.
- `Option` fields are optional properties that may also be `null`.
- A WebSocket message is capped at 8 MiB: send files through `/upload`, never as JSON bytes.

## Cloud runners through this computer

The browser only ever talks to this computer's runtime. To follow a cloud runner, the client wraps
messages as `{"type":"via_runner","runner":N,"message":…}`; the runtime opens the line to that runner
and answers with the same wrapper, and the runner's terminal bytes arrive as frame kind 3
(`u64 runner`, `u64 terminal`, bytes). `client.relay(runner)` hides all of this: it returns a
`RuntimeClient` that behaves like a direct one, reconnects with its parent and gets the runner's own
snapshot. Permissions are the intersection of the browser's pairing and the runner's. Only
`hello`, `subscribe`, `ask_snapshot` and commands are relayed.

## Notifications

`GET /push` gives the runtime's VAPID public key, `POST /push` keeps a browser's push subscription
(only Apple, Google, Mozilla and Microsoft push services are accepted) and `DELETE /push` with
`{"endpoint"}` forgets it; all need a paired browser. The runtime sends a notification when a chat
goes from working to waiting or done, with `{title, body, tag, url}`, `url` being a `#s=` address.
