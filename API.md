# Krad Agents API v1

The Krad Agents API is the protocol of Krad ADE's agents bus. It is how agents running in the app's
terminals and chats list each other, wait for each other and, from a lead, start and close
Workers; how scripts and extensions read the app's Kraddies, automations, sessions and changes;
and how programs outside the app watch every agent's state as it changes. The `krad` CLI speaks it,
and so can any program that opens a TCP socket and writes a line of JSON.

The machine-readable description, whose every example the test suite reads and writes back
byte for byte, is [`sdk/schema/krad-agents-api.json`](sdk/schema/krad-agents-api.json). The words
used here (session, group, lead, Worker, Kraddie, keepsake) are defined in
[`CONTEXT.md`](CONTEXT.md).

## Contents

- [Who it is for](#who-it-is-for)
- [Transport](#transport)
- [The envelope and the handshake](#the-envelope-and-the-handshake)
- [Authentication and callers](#authentication-and-callers)
- [Finding the endpoint](#finding-the-endpoint)
- [Security model](#security-model)
- [Requests](#requests)
  - [Agents](#agents)
  - [Messages and screens](#messages-and-screens)
  - [Workers](#workers)
  - [Sessions, worktrees, files and changes](#sessions-worktrees-files-and-changes)
  - [Kraddies and automations](#kraddies-and-automations)
  - [Memory](#memory)
  - [Variants](#variants)
  - [Requests the app's integrations use](#requests-the-apps-integrations-use)
- [Agent states](#agent-states)
- [Subscriptions](#subscriptions)
- [App events, the event journal and webhooks](#app-events-the-event-journal-and-webhooks)
- [Errors and exit codes](#errors-and-exit-codes)
- [The krad CLI](#the-krad-cli)
- [Cookbook](#cookbook)
- [Compatibility and stability](#compatibility-and-stability)

## Who it is for

| Client | Reaches the bus through | Can |
|---|---|---|
| An agent or shell in one of the app's terminals | `KRAD_ADE_*` variables the app sets in that terminal | List and wait for the agents of its environment, read their screens, use sessions, worktrees, files, changes, Kraddies, automations, memory and variants |
| A lead (the chat of a session the user made its orchestrator) | Its terminal's variables; the app recognises it | All of the above, plus start, direct and close its Workers, close sessions of its group and pick a variant winner |
| A script, extension or tool outside the app | The [API discovery file](#finding-the-endpoint) | List and watch the agents of every environment |
| A webhook receiver | An address added in Settings › Webhooks | Receive [app events](#app-events-the-event-journal-and-webhooks) as signed HTTP posts |

## Transport

- **Endpoint.** A TCP listener on `127.0.0.1` with a port the system picks, opened when the app
  starts. It is never bound to another interface. Terminals on an SSH host reach it through a
  reverse port forward the app opens on that host.
- **One request per connection.** The client writes one envelope as a single line of JSON ending in
  `\n`. The app answers with one line of JSON and closes the connection.
- **Streaming.** `SubscribeAgents` is the exception: the connection stays open and the app writes
  one JSON line per message until either side closes it. See [Subscriptions](#subscriptions).
- **Limits.** A request line may be up to 64 KiB, and must arrive within 10 seconds of connecting;
  longer or slower lines are answered with `bad_request`.
- **Deadlines.** Each request has a deadline after which the app answers `"TimedOut"`: 10 seconds
  for most requests, 1 minute for worktree, changes, close and variant picks, 1 minute for starting
  a Worker or a variants run, 2 minutes for code intelligence and extension tools. `Wait` and
  `Check` with a wait last as long as asked, up to 30 minutes, plus one second.
- **Leaving cancels.** A client that closes its connection while a request waits (a `Wait`, a
  `Check --wait`) cancels it.

## The envelope and the handshake

Every request line is an envelope:

```json
{"api_version":1,"token":"6f1c0d9e2b7a4c58e3f09a1d7b6c2e40","from":"Client","request":"Hello"}
```

| Field | Meaning |
|---|---|
| `api_version` | The API version the client speaks: `1`. A missing field is read as `1`. |
| `token` | The bus token, from `KRAD_ADE_TOKEN` (or the file `KRAD_ADE_TOKEN_FILE` names) inside the app's terminals, or from `api.json` outside them. |
| `from` | Who is calling: `"Client"` outside the app, `{"Terminal":{"environment":"main","terminal":4}}` from a terminal. See [callers](#authentication-and-callers). |
| `request` | One of the [requests](#requests). Requests without fields are a string (`"List"`); the others an object with one key (`{"Wait":{"target":4,"timeout_secs":600}}`). |

Responses follow the same shape: `{"Agents":[...]}`, `{"Error":{"code":"not_found","message":"..."}}`,
or a bare string such as `"TimedOut"`.

**Handshake.** `"Hello"` answers the version the app serves, its build and its process id. The bus
answers it itself, so it works even while the app is busy:

```json
{"Hello":{"api_version":1,"build":"0.3.3","pid":48211}}
```

**Order of checks.** The app reads the line (`bad_request` when it is not a valid envelope), checks
the token (`unauthorized`), then the version: an envelope whose `api_version` is not the one the
app serves is refused before anything else, `Hello` included:

```json
{"Error":{"code":"unsupported_api_version","message":"This app serves the agents API v1, not v2"}}
```

The `krad` CLI exits with code 6 when that happens.

## Authentication and callers

The app makes two random 128-bit tokens, written as 32 hex digits, each time it starts, and
compares them in constant time.

- **The bus token** is handed to every terminal the app opens (as `KRAD_ADE_TOKEN`, or as a file
  named by `KRAD_ADE_TOKEN_FILE` on SSH hosts, created owner-only) and written to `api.json`.
- **The lead token** is kept by the app. An envelope whose `from` is `{"Lead":...}` must carry it;
  with the bus token it is refused with `unauthorized` ("only a lead the user named can call as a
  lead"). No terminal receives it: a lead calls as its own terminal and the app recognises it.

`from` names the caller, and the app answers according to it:

| `from` | Who | Scope and rights |
|---|---|---|
| `"Client"` | A program outside the app's terminals | `Hello`, `List` and `ListAll` across every open environment, and `SubscribeAgents`. Anything else is refused with `forbidden`. |
| `{"Terminal":{"environment":E,"terminal":N}}` | A terminal or chat of environment `E`; the app sets `KRAD_ADE_TERMINAL=E:N` | Its own environment and group. Every request except those that direct Workers. |
| The same, when terminal `N` is a session's lead chat | A lead | For `Spawn`, `Send`, `Close`, `CloseSession` and `PickVariant`, and for `Read` and `Wait` on its own Workers, the app treats it as the lead of that session. Leads are the only callers allowed to direct Workers; anyone else gets `forbidden`. |
| `{"Git":"main"}` | The app's own `git` credential prompts | Internal; only the askpass request. |

The CLI fills `from` from `KRAD_ADE_TERMINAL` inside the app, and with `"Client"` when it found the
endpoint through `api.json`.

## Finding the endpoint

Inside the app's terminals and chats the app sets:

| Variable | Value |
|---|---|
| `KRAD_ADE_ENDPOINT` | `127.0.0.1:<port>` (on an SSH host, the forwarded address there) |
| `KRAD_ADE_TOKEN` | The bus token (on this computer) |
| `KRAD_ADE_TOKEN_FILE` | The owner-only file holding the token (on SSH hosts, instead of `KRAD_ADE_TOKEN`) |
| `KRAD_ADE_TERMINAL` | The caller handle, `environment:terminal` |
| `PATH` | Prefixed with the app's own folder, where `krad` and `krad-ade-agents` live |

Outside them, the app leaves an **API discovery file**, `api.json`, in its config folder
(`$XDG_CONFIG_HOME/krad-ade`, else `~/.config/krad-ade`, else `%APPDATA%\krad-ade`):

```json
{
  "api_version": 1,
  "endpoint": "127.0.0.1:52144",
  "token": "6f1c0d9e2b7a4c58e3f09a1d7b6c2e40",
  "pid": 48211,
  "started_at": "2026-11-10T09:12:00.000Z"
}
```

- It is written atomically (a temporary file renamed into place) and owner-only (`0600` on Unix),
  replaced each time the app starts and removed when it exits cleanly, but only by the process
  whose `pid` it names.
- **Settings › General › "Let other programs find the agents API"** turns it on and off; it is
  on by default and turning it off removes the file at once.
- **Staleness.** A file left by a crash can name a dead endpoint, or a port another program now
  holds. A file is live when `Hello` at its endpoint answers with the same `pid`; it is stale when
  nothing answers, or when another process does. `krad api info` makes that check and exits with 2
  on a stale file.
- The variables win over the file: the CLI reads `api.json` only when `KRAD_ADE_ENDPOINT` and
  `KRAD_ADE_TERMINAL` are not set. The static Linux build of the CLI that runs on SSH hosts has no
  discovery file and only uses the variables.
- The app serves the bus only when it finds its `krad-ade-agents` CLI next to its own executable or
  on `PATH`.

`krad api info` prints where the API is and how it was found; `--show-token` adds the token and
`--json` prints `{"endpoint","api_version","build","discovery_file","token"}` (the last two only
when they apply).

## Security model

- **Loopback only.** Nothing outside this computer can connect, except through the SSH forwards
  the app itself opens to the hosts of its environments.
- **A token per run.** Every request needs the bus token. It reaches only the app's terminals and
  the owner-only `api.json`, it changes each time the app starts, and it is never logged: types
  holding it print `[REDACTED]`.
- **The caller is declared, not proven.** `from` is what the client says it is. The token proves
  the client runs as the user (it could read the file or the terminal's environment); it does not
  prove which terminal sent the request. A program holding the token could name any terminal of
  any environment, a lead's included. The lead token keeps the `Lead` caller itself out of reach,
  but the rights the app gives a lead's own terminal follow from the name.
- **Narrow rights outside.** A `"Client"` caller can only list and watch agents. Turning the
  discovery file off leaves the API to the app's own terminals.
- **No contents on the wire by default.** Agent events and app events carry ids, titles and states,
  never terminal contents. Reading a screen (`Read`) or a group's patch (`Changes`) needs a
  terminal caller of that environment.

The trade-off is convenience over isolation from other processes of the same user: anything that
runs as you can already read your files and your terminals' environments, and the API does not try
to protect the app from it. It protects the app from other users and other machines.

## Requests

Every example below is the `request` field of an envelope; the line under it is the response.
Fields marked optional may be left out. Terminal ids, session ids and run ids are numbers; names are
strings.

### Agents

**`List`**: the agents (terminals and chats, not file or extension tabs) of the caller's
environment; for a `"Client"`, of every open environment.

```json
"List"
{"Agents":[{"environment":"main","terminal":4,"title":"Claude Code · 4","group":"api","state":"working","harness":"Claude Code","is_self":false}]}
```

| Field | Meaning |
|---|---|
| `environment` | The environment's name. Omitted in some answers to a terminal, which only sees its own environment. |
| `terminal` | The terminal id, unique within the environment. |
| `title` | The tab's title. |
| `group` | The group's name, or `null`. |
| `state` | `idle`, `working`, `waiting-for-input`, `done`, `exited` or `closed`. See [Agent states](#agent-states). |
| `harness` | `Shell`, `Claude Code`, `Codex`, `opencode`, `Gateway`, or `acp:<id>` for an ACP agent; `null` when unknown. |
| `is_self` | `true` on the caller's own terminal. |

**`ListAll`**: `List` plus the Workers that were closed, with state `closed`.

**`SubscribeAgents`**: a stream of agent events. See [Subscriptions](#subscriptions).

### Messages and screens

**`Wait`**: blocks until a terminal stops working and answers its settled state, or `"TimedOut"`.
`timeout_secs` is capped at 1800. A terminal the app is still starting counts as working.

```json
{"Wait":{"target":4,"timeout_secs":600}}
{"Settled":{"target":4,"state":"done"}}
```

**`Read`**: the last `lines` lines of a terminal's screen, or of a chat's conversation.

```json
{"Read":{"target":4,"lines":40}}
{"Screen":"..."}
```

A lead that reads one of its Workers after the Worker has finished closes it, as `Close` would;
reading a closed Worker answers `not_found` ("send it a message to reopen it").

**`Check`**: the caller's unread mailbox messages; with `wait_secs`, blocks until one arrives
(capped at 1800 seconds). A lead's chat has no mailbox and gets `unsupported`.

```json
{"Check":{"wait_secs":600}}
{"Messages":[{"id":3,"from":4,"from_title":"Claude Code · 4","body":"API done"}]}
```

**`Send`** (lead only): types `body` into a Worker as its next message, as the user would. A closed
Worker reopens with its conversation first.

```json
{"Send":{"to":12,"body":"Add a test for the empty case"}}
{"Submitted":{"terminal":12}}
```

### Workers

**`Spawn`** (lead only): starts a Worker in a session of its own in the lead's group, as a chat, and
answers once it has started. `agent` is a harness program (`claude`, `codex`, `opencode`) or
`acp:<id>`; `worktree`, optional, names a new git worktree to run it in.

```json
{"Spawn":{"agent":"codex","worktree":"fix-login","task":"Fix the login redirect. Done when ..."}}
{"Spawned":{"terminal":12}}
```

**`Close`** (lead only): closes Workers or a session.

```json
{"Close":{"target":{"agent":12},"when_working":"refuse","worktree":"keep"}}
{"Closed":[{"target":{"agent":12},"worktree":{"path":"/src/app.worktrees/fix-login","outcome":"kept"}}]}
```

| Field | Values |
|---|---|
| `target` | `{"agent":N}`, `{"session":N}` or `"all_idle"` (every Worker of the lead that has finished) |
| `when_working` | `"refuse"` (default): a Worker still working or waiting for input is refused with `busy`. `"cancel_turn"`: its turn is stopped first. |
| `worktree` | `"keep"` (default) or `"remove"`: removes the Worker's worktree only when it has no uncommitted changes and no commits missing from the repository's checked-out branch, and no other session uses it. |

Each entry of `Closed` names what closed and, when it had one, where its worktree is and what
happened to it: `"kept"`, `"removed"`, or `{"refused":{"code":"worktree_not_clean","message":"..."}}`.
An empty list means there was nothing to close.

Ownership rules, each with its code:

| Refusal | Code |
|---|---|
| No Worker or session with that id | `not_found` |
| The Worker is already closed | `not_found` |
| Not one of the lead's Workers, or a session outside its group | `not_owned` |
| The lead's own session | `forbidden` |
| Still working, without `cancel_turn` | `busy` |
| A session of an environment that is not the one on screen | `unsupported` |
| The caller is not a lead | `forbidden` |

The closed Worker's chat is kept, and a later `Send` reopens it.

### Sessions, worktrees, files and changes

These act in the caller's environment and group.

**`Sessions`**

```json
"Sessions"
{"Sessions":[{"session":2,"name":"API","group":"api","is_current":true,"terminals":[4,7]}]}
```

**`CreateSession`**: opens a session in the caller's group, or the group named `group`, without
taking the user's place on screen. Both fields are optional.

```json
{"CreateSession":{"name":"Docs","group":null}}
{"SessionCreated":{"session":5}}
```

**`CloseSession`** (lead only): closes a session of the lead's group and the Workers it holds. The
last session of a group that still has tabs cannot be closed (`failed`).

```json
{"CloseSession":{"session":5}}
{"SessionClosed":{"session":5}}
```

**`CreateWorktree`**: a git worktree of the group's repository on a new branch, opened as a group of
its own.

```json
{"CreateWorktree":{"name":"fix-login"}}
{"WorktreeCreated":{"group":"fix-login","path":"/src/app.worktrees/fix-login","branch":"fix-login"}}
```

**`OpenFile`**: opens a file of the caller's group in the app's editor. `path` is relative to the
group's folder; `position` is optional, 1-based, with an optional `column`. Paths that climb out of
the folder, directly or through a link, are refused with `forbidden`.

```json
{"OpenFile":{"path":"src/main.rs","position":{"line":12,"column":5}}}
{"Opened":{"path":"/src/app/src/main.rs"}}
```

**`Changes`**: the git changes of a session's group (the caller's when `session` is `null`), with
the patch against `HEAD`.

```json
{"Changes":{"session":null}}
{"Changes":{"group":"api","root":"/src/app","branch":"main","files":[{"path":"src/lib.rs","kind":"modified"}],"patch":"diff --git ..."}}
```

`kind` is `modified`, `added`, `deleted`, `renamed`, `untracked` or `conflicted`.

### Kraddies and automations

Kraddies and their automations belong to the user, not to an environment, so these answer the same
from every terminal. `name` matches a Kraddie's name or id.

| Request | Response |
|---|---|
| `"Kraddies"` | `{"Kraddies":[{"id":"owl","name":"Owl","switch":"on","harness":"Claude Code","schedule":"...","next_run":"...","last_outcome":"..."}]}`; `schedule`, `next_run` and `last_outcome` are omitted when they do not apply. |
| `{"RunKraddie":{"name":"owl","prompt":null}}` | `{"KraddieStarted":{"kraddie":"owl","run":4}}`: the Kraddie wakes and runs its manual automation, or its first one. |
| `"Automations"` | `{"Automations":[{"kraddie":"owl","id":1,"name":"Docs","switch":"on","triggers":["..."],"next_run":"...","last_outcome":"...","runs":3}]}` |
| `{"RunAutomation":{"name":"owl/Docs","prompt":"Only the API folder"}}` | `{"AutomationStarted":{"kraddie":"owl","automation":"Docs","run":7}}`. Write `<kraddie>/<name>` when two Kraddies have an automation of that name, or the answer is `bad_request`. |
| `{"SwitchAutomation":{"name":"Docs","switch":"off"}}` | `{"AutomationSwitched":{...}}` with the automation as `Automations` lists it. |
| `{"RememberKraddie":{"name":"owl","text":"CI uses runner 7"}}` | `{"KraddieMemory":{"kraddie":"owl","lines":["CI uses runner 7"],"limit":200}}`: hands the Kraddie a keepsake. |
| `{"KraddieMemory":{"name":"owl"}}` | The same answer: the keepsakes it carries and how many it may keep. |
| `{"NewKraddieChat":{"name":"owl"}}` | `{"KraddieChatOpened":{"kraddie":"owl","session":9,"terminal":31}}`: a new chat that belongs to the Kraddie. |

`next_run` and `last_outcome` are display text for people, not timestamps to parse.

### Memory

A Kraddie remembers keepsakes (short lines it carries into every run) and chat digests (facts the
app draws from its chats). Ordinary chats, those of no Kraddie, have digests of their own.

**`KraddieRecall`**: a Kraddie's keepsakes, digests, the Kraddies it reads and whether its digests
are automatic. `view` is `"list"` (default, may be left out) or `"show"`; the CLI prints every fact
with `show`.

```json
{"KraddieRecall":{"name":"owl","view":"show"}}
{"KraddieRecall":{"kraddie":"owl","keepsakes":["CI uses runner 7"],"digests":[{"id":"kraddie/owl/digest/3","chat":"Release notes","facts":["Releases are tagged from main"]}],"reads":["fox"],"automatic_digests":true}}
```

**`ShareKraddieMemory`**: lets `reader` read the memory of `from` (`"on"`) or stops it (`"off"`).
Sharing has a direction: the Kraddie read from does not read its reader back. A Kraddie always reads
its own memory (`bad_request`).

```json
{"ShareKraddieMemory":{"reader":"owl","from":"fox","switch":"on"}}
{"KraddieShared":{"kraddie":"owl","reads":["fox"]}}
```

**`MemorySearch`** and **`MemoryRead`**: search the memory the caller may read, then read one entry in
full. `limit` is optional: 8 by default, between 1 and 30.

```json
{"MemorySearch":{"query":"staging","limit":8}}
{"MemoryHits":[{"id":"chats/digest/1","title":"Staging","excerpt":"Staging deploys from the main branch"}]}

{"MemoryRead":{"id":"kraddie/fox/keepsake/2"}}
{"MemoryEntry":{"id":"kraddie/fox/keepsake/2","text":"..."}}
```

Memory ids are `kraddie/<id>/keepsake/<n>` (1-based), `kraddie/<id>/digest/<n>` and
`chats/digest/<n>`. A malformed id is `bad_request`, an unknown one `not_found`.

**Visibility.** What a caller may read is decided on every call:

- Everyone may read the digests of ordinary chats.
- A chat that belongs to a Kraddie reads that Kraddie's memory and the memory of the Kraddies it
  reads.
- A chat also reads the Kraddies the user mentioned in it with `$`.
- Anything else is refused: `MemoryRead` with `forbidden`, `KraddieMemory` and `KraddieRecall` of a
  Kraddie out of reach with `out_of_reach`.
- Only the user changes memory: `RememberKraddie` and `ShareKraddieMemory` from a chat are refused
  with `forbidden`; they work from the user's own terminal and in the app.

**The memory MCP tools.** The app's MCP server, `krad mcp` (server name `krad-code`), offers chats
two read-only tools, `memory_search` (`query`, optional `limit` from 1 to 30) and `memory_read`
(`id`), which make the two requests above with the same visibility rules. They are listed only
while there is a Kraddie or an ordinary chat digest to read.

### Variants

A variants run gives one task to two to four variants, each a harness with an optional model and
extra instruction, in a worktree of its own, to compare them in the app's Variants view.

**`RunVariants`**: `test`, optional, is a project task such as `cargo test` run in each worktree once
its variant finishes; `group`, optional, names the group (the caller's by default). `harness` is
`ClaudeCode`, `Codex`, `Opencode` or `Acp:<id>`.

```json
{"RunVariants":{"prompt":"Fix the flaky upload test","variants":[{"harness":"Codex","model":"gpt-5.5"},{"harness":"ClaudeCode","instruction":"Keep the diff small"}],"test":"cargo test"}}
{"VariantsStarted":{"run":4}}
```

**`Variants`**: every run of the group, or the run named; each with its variants' state, cost,
tokens, time, test result and diff. The shape of each run follows the app's own record of it and
is not part of the stable surface; read it for display.

```json
{"Variants":{"run":4}}
```

**`PickVariant`** (lead only): keeps the winner and discards the other variants and their worktrees.
`outcome` is `"keep_worktree"` (the winner's worktree becomes its session) or `"apply_onto_base"`
(its changes are applied onto the base worktree). `confirmed` must be `true`. A run of another
group is `not_owned`.

```json
{"PickVariant":{"run":4,"variant":1,"outcome":"keep_worktree","confirmed":true}}
{"VariantPicked":{"run":4,"variant":1}}
```

### Requests the app's integrations use

`Hook`, `Status`, `Ask` and `Askpass` carry harness hooks, status lines, permission asks and `git`
credential prompts from the processes the app starts; `Code`, `Tools` and `ExtensionTool` serve the
code intelligence, the tool catalog and the extension tools behind `krad code`, `krad tools`,
`krad tool` and `krad mcp`. They are part of the protocol so the app's own pieces can use it, but
their payloads follow the vendors' formats and may change with them; programs should not depend on
them.

## Agent states

| State | Meaning | Coarse state |
|---|---|---|
| `idle` | Open, not working, nothing to see | `idle` |
| `working` | In the middle of a turn | `working` |
| `waiting-for-input` | Waiting for the user: a permission, a question or input | `waiting-for-input` |
| `done` | Finished its turn and the user has not looked yet | `idle` |
| `exited` | Its process ended; the tab is still there | `closed` |
| `closed` | The tab or Worker is gone | `closed` |

`coarse_state` folds the six states into four for clients that only care whether an agent is busy,
needs someone or is gone.

## Subscriptions

```json
{"api_version":1,"token":"…","from":"Client","request":{"SubscribeAgents":{"filter":{"group":"api"}}}}
```

`SubscribeAgents` takes three optional fields:

| Field | Meaning |
|---|---|
| `filter` | Any of `environment`, `group`, `harness` (a display name such as `"Claude Code"`, or `acp:<id>`) and `terminals` (a list of ids). Every field given must match; an empty filter, or none, admits every agent of every environment. |
| `since_seq` | The last `seq` the client saw, to resume. |
| `stream` | The stream that `seq` came from. |

**Lines.** The app writes, one per line:

- `AgentSnapshot`: `{"stream","seq","at","resync","agents":[...]}`, the open agents the filter admits,
  in the shape `List` uses.
- `AgentEvent`: one change of one agent.
- `Heartbeat`: `{"stream","seq","at"}`, written when nothing else was written for 15 seconds.
- `Error`: the last line before the app closes the stream.

```json
{"AgentEvent":{"seq":42,"at":"2026-11-10T09:15:02.481Z","kind":"state_changed","environment":"main","terminal":4,"title":"Claude Code · 4","group":"api","harness":"Claude Code","previous":"working","state":"waiting-for-input","coarse_state":"waiting-for-input","reason":"permission_pending"}}
```

| Field | Meaning |
|---|---|
| `seq` | Grows by one per event within a stream. A snapshot and a heartbeat carry the `seq` of the last event before them. |
| `stream` | Identifies one run of the app; `seq` starts again in a new stream. |
| `at` | RFC 3339 in UTC with milliseconds. |
| `kind` | `opened` (`previous` is `null`), `state_changed`, or `closed` (`state` is `closed`). |
| `previous`, `state`, `coarse_state` | The states before and after. |
| `reason` | Present when known: `turn_started`, `turn_finished`, `ask_pending`, `permission_pending`, `input_pending`, `process_exited`, `user_closed` or `environment_closed`. |

Only changes of state are events: a new title alone does not make one.

**First line and resuming.**

- Without `since_seq`, the first line is an `AgentSnapshot` with `resync: false`.
- With `since_seq` and the `stream` it came from, when that stream is the app's current one and the
  `seq` is still within the last 1024 events, the first line is a `Heartbeat`, followed by the
  missed events the filter admits.
- Otherwise (another stream, a `seq` too old or in the future, or `since_seq` without `stream`) the
  first line is an `AgentSnapshot` with `resync: true`: replace what you knew with it.

**Slow subscribers.** Each subscriber has a queue of 256 lines. A subscriber that lets it fill is
dropped: it receives the lines already queued, then
`{"Error":{"code":"slow_subscriber","message":"..."}}`, and the connection closes. Subscribe again
with `since_seq` and `stream`. When the app stops publishing, the last line is an `Error` with code
`unavailable`.

**Liveness.** A client that hears nothing, not even a heartbeat, for three intervals (45 seconds)
should assume the app is gone; `krad agents watch` exits with 2 then.

**Example session** (one line per message; the first is the client's):

```json
{"api_version":1,"token":"6f1c…","from":"Client","request":{"SubscribeAgents":{"filter":{"group":"api"}}}}
{"AgentSnapshot":{"stream":"9d3b5f0a7c214e6b8a90c1d2e3f4a5b6","seq":41,"at":"2026-11-10T09:15:00.120Z","resync":false,"agents":[{"environment":"main","terminal":4,"title":"Claude Code · 4","group":"api","state":"working","harness":"Claude Code","is_self":false}]}}
{"AgentEvent":{"seq":42,"at":"2026-11-10T09:15:02.481Z","kind":"state_changed","environment":"main","terminal":4,"title":"Claude Code · 4","group":"api","harness":"Claude Code","previous":"working","state":"waiting-for-input","coarse_state":"waiting-for-input","reason":"permission_pending"}}
{"AgentEvent":{"seq":43,"at":"2026-11-10T09:15:09.007Z","kind":"state_changed","environment":"main","terminal":4,"title":"Claude Code · 4","group":"api","harness":"Claude Code","previous":"waiting-for-input","state":"working","coarse_state":"working","reason":"turn_started"}}
{"AgentEvent":{"seq":44,"at":"2026-11-10T09:16:30.550Z","kind":"opened","environment":"main","terminal":7,"title":"Codex · 7","group":"api","harness":"Codex","previous":null,"state":"idle","coarse_state":"idle"}}
{"Heartbeat":{"stream":"9d3b5f0a7c214e6b8a90c1d2e3f4a5b6","seq":44,"at":"2026-11-10T09:16:45.550Z"}}
{"AgentEvent":{"seq":45,"at":"2026-11-10T09:18:12.904Z","kind":"state_changed","environment":"main","terminal":4,"title":"Claude Code · 4","group":"api","harness":"Claude Code","previous":"working","state":"done","coarse_state":"idle","reason":"turn_finished"}}
{"AgentEvent":{"seq":46,"at":"2026-11-10T09:18:40.033Z","kind":"closed","environment":"main","terminal":4,"title":"Claude Code · 4","group":"api","harness":"Claude Code","previous":"done","state":"closed","coarse_state":"closed","reason":"user_closed"}}
```

## App events, the event journal and webhooks

Besides agent events, the app publishes **app events**: typed facts about its own state. They name
ids, never contents, and go to the group's event journal, to the webhooks that chose them and to
the extensions subscribed to them.

**Record.** One JSON object per event:

```json
{"api":1,"at":"2026-11-10T09:15:02.481Z","environment":"main","group":2,"event":{"kind":"agent_started","terminal":7,"harness":"ClaudeCode"}}
```

`api` is the version of the event format (1); `at` is RFC 3339 in UTC with milliseconds; `group`
is left out for events of no group. Readers skip records of a newer `api` or an unknown `kind`.

| `kind` | Other fields | When |
|---|---|---|
| `agent_started` | `terminal`, `harness` (optional) | An agent starts a turn |
| `agent_waiting` | `terminal` | An agent waits for input |
| `agent_finished` | `terminal` | An agent finishes |
| `worktree_created` | `from` (the group it came from) | A worktree group opens |
| `worktree_removed` | | A worktree is removed |
| `session_opened` | `session` | A session opens |
| `session_closed` | `session` | A session closes |
| `git_changed` | | The group's git status changes |
| `review_sent` | `merge_request`, `verdict` (`commented`, `replied`, `approved`) | A review is sent |
| `ci_failed`, `ci_passed` | `merge_request`, `pipeline` | A pipeline of an open merge or pull request ends |
| `merge_request_opened` | `merge_request` | A merge or pull request opens |
| `variants_started` | `run`, `variants` (how many) | A variants run starts |
| `variants_finished` | `run` | Every variant of a run is done |
| `variant_winner_chosen` | `run`, `variant`, `outcome` | A winner is picked |
| `ping` | | The test event Settings sends |

`harness` here is `Shell`, `ClaudeCode`, `Codex`, `Opencode`, `Gateway` or `Acp:<id>`. The code
host events are found by asking GitLab or GitHub every two minutes about the group's open merge
and pull requests (up to 20 per project); the app never opens an address for them to call.

**Event journal.** Each group's events are appended to
`<config folder>/events/<environment>/group-<id>.jsonl` (events of no group to `environment.jsonl`),
one record per line. A file that would pass 512 KiB rotates to `.1.jsonl`, `.2.jsonl` and
`.3.jsonl`, the oldest dropped.

**Webhooks.** Settings › Webhooks adds an `http` or `https` address, a name, the event kinds it
wants and, optionally, a secret kept in the system keyring (or an owner-only file in the config
folder). For each event it chose, the app sends one `POST` with the record as the body:

| Header | Value |
|---|---|
| `content-type` | `application/json` |
| `user-agent` | `krad-ade-webhooks` |
| `x-krad-timestamp` | When this attempt was sent, RFC 3339 in UTC with milliseconds |
| `x-krad-signature-256` | With a secret: `sha256=` and the lowercase hex HMAC-SHA256 of `<timestamp>.<body>` |

- A `2xx` answer delivers it. A `4xx` other than 408 and 429 gives up at once. Anything else
  (408, 429, `3xx`, `5xx`, no answer within 10 seconds) is retried, up to five attempts in all,
  waiting 2, 4, 8 and 16 seconds between them (never more than 60).
- A retry goes to the webhook as it is then: edited, it uses the new address; removed, it is dropped.
- A webhook marked as signed whose secret is missing sends nothing.
- The "Test event" button sends one `ping`, whatever kinds the webhook chose.

**Verifying a signature.** Recompute the HMAC over the timestamp header, a dot and the raw body,
compare in constant time, and turn away timestamps too far from now to stop replays.

```bash
expected="sha256=$(printf '%s.%s' "$timestamp" "$body" \
  | openssl dgst -sha256 -hmac "$KRAD_WEBHOOK_SECRET" -hex | sed 's/^.* //')"
[ "$expected" = "$signature" ] && echo valid
```

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

fn is_signed_by(secret: &[u8], timestamp: &str, body: &[u8], header: &str) -> bool {
    let Some(hex) = header.strip_prefix("sha256=") else { return false };
    let Ok(expected) = hex::decode(hex) else { return false };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else { return false };
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}
```

## Errors and exit codes

Every refusal is `{"Error":{"code":"…","message":"…"}}`. Codes are stable; messages are for people
and may change.

| Code | Meaning | CLI exit |
|---|---|---|
| `bad_request` | The line or a field is malformed, or a name is ambiguous | 1 |
| `unavailable` | The app stopped serving, dropped the request or the caller left | 2 |
| `slow_subscriber` | A subscriber fell 256 lines behind and was dropped | 2 |
| `not_found` | No such agent, session, Kraddie, automation, run, group, file or memory | 4 |
| `unauthorized` | Wrong token, or a `Lead` caller without the lead token | 5 |
| `forbidden` | Not allowed for this caller (not a lead, a client, a path outside the group, memory out of reach for reading) | 5 |
| `unsupported` | Not possible here (a lead's mailbox, another environment than the one on screen, a caller with no group) | 5 |
| `not_owned` | Not one of the lead's Workers, or a session or run outside its group | 5 |
| `busy` | Still working; close with `cancel_turn` | 5 |
| `out_of_reach` | A Kraddie's memory this chat may not read | 5 |
| `unsupported_api_version` | The app serves another API version | 6 |
| `failed` | The app tried and could not do it | 7 |
| `worktree_not_clean` | A worktree with uncommitted or unmerged work was kept | 7 |

The CLI's exit codes:

| Exit | Meaning |
|---|---|
| 0 | Done |
| 1 | Wrong arguments |
| 2 | The app cannot be reached (also a stale discovery file, a dropped slow subscriber or a missed heartbeat) |
| 3 | Timed out |
| 4 | No such agent, session or file |
| 5 | Refused for this caller |
| 6 | The app speaks another API version |
| 7 | The app could not do it |

## The krad CLI

`krad` and `krad-ade-agents` are the same program; the app puts both on the `PATH` of its
terminals, and its help names the one you typed. `--json` on any command prints the response line
instead of text (for `status`, an object with `working`, `waiting_for_input`, `done`, `idle` and
`exited` lists), and `--version` prints the build. `krad help lead` prints the lead's guide.

| Command | Request |
|---|---|
| `krad list [--all]` | `List`, or `ListAll` |
| `krad agents list [--all]` | The same |
| `krad agents watch [--group G] [--harness H] [--terminal ID]... [--since SEQ --stream ID]` | `SubscribeAgents`; prints the snapshot, whose first line names its seq and stream, then one line per event until interrupted. `--harness` takes `claude`, `codex`, `opencode`, `acp:<id>` or a display name. |
| `krad status` | `List`, grouped by state; agents of unknown harness are left out |
| `krad send <id> <message>` | `Send` |
| `krad check [--wait SECS]` | `Check` |
| `krad wait <id> [--timeout SECS]` | `Wait`; 600 seconds by default, at most 1800 |
| `krad read <id> [--lines N]` | `Read`; 40 lines by default |
| `krad spawn --agent <claude\|codex\|opencode\|acp-id> [--worktree NAME] <task>` | `Spawn` |
| `krad close <id> [--force] [--remove-worktree]` | `Close` an agent; `--force` is `cancel_turn`, `--remove-worktree` is `remove` |
| `krad close --session <id> [--force] [--remove-worktree]` | `Close` a session |
| `krad close --all-idle [--remove-worktree]` | `Close` every finished Worker |
| `krad sessions` | `Sessions` |
| `krad session create [NAME] [--group GROUP]` | `CreateSession` |
| `krad session close <id>` | `CloseSession` |
| `krad worktree create <name>` | `CreateWorktree` |
| `krad open <file>[:LINE[:COLUMN]]` | `OpenFile` |
| `krad changes [<session id>]` | `Changes`, as a list of files |
| `krad diff [<session id>]` | `Changes`, as the patch |
| `krad kraddie list` | `Kraddies` |
| `krad kraddie run <name> [prompt]` | `RunKraddie` |
| `krad kraddie remember <name> <text>` | `RememberKraddie` |
| `krad kraddie memory <name>` | `KraddieMemory` |
| `krad kraddie memory list\|show <name>` | `KraddieRecall` |
| `krad kraddie chat new <name>` | `NewKraddieChat` |
| `krad kraddie share\|unshare <reader> <read-from>` | `ShareKraddieMemory` |
| `krad automation list` | `Automations` |
| `krad automation run <name> [prompt]` | `RunAutomation` |
| `krad automation enable\|disable <name>` | `SwitchAutomation` |
| `krad variants run --variant <agent> [--model M] [--instruction TEXT] ... [--test TASK] [--group G] <task>` | `RunVariants`; two to four `--variant`, each followed by its own `--model` and `--instruction` |
| `krad variants status [<run id>]` | `Variants` |
| `krad variants pick <run id> <variant> [--apply] --yes` | `PickVariant`; refused without `--yes` |
| `krad code <diagnostics\|definition\|references\|hover\|rename> FILE[:LINE:COLUMN] [NEW_NAME]` | `Code` |
| `krad tools`, `krad tool <name> [JSON arguments]` | `Tools`, `ExtensionTool` |
| `krad api` | `Hello`: the API version the app serves |
| `krad api info [--show-token]` | `Hello` and the [staleness check](#finding-the-endpoint) |
| `krad help [lead]` | The guide |

`krad pet` still runs `krad kraddie`, from the feature's former name; it is not listed in the help.
The verbs `hook`, `ask`, `statusline` and `mcp` are run by the harnesses the app configures, not by
people, and are not listed either.

## Cookbook

**Poll the agents.**

```bash
krad agents list --json | jq -r '.Agents[] | "\(.environment):\(.terminal) \(.state) \(.title)"'
```

**Watch them.**

```bash
krad agents watch --group api --json | while read -r line; do
  echo "$line" | jq -r 'select(.AgentEvent) | .AgentEvent | "\(.terminal) \(.previous) -> \(.state)"'
done
```

To resume after a restart of your script, keep the last `seq` and the `stream` of the first line,
and pass `--since SEQ --stream ID`.

**Wait for an agent, then read it.**

```bash
krad wait 4 --timeout 900
case $? in
  0) krad read 4 --lines 60 ;;
  3) echo "still working" ;;
esac
```

**Close the Workers that finished** (from a lead):

```bash
krad close --all-idle --remove-worktree --json
```

**Receive webhooks.** Add the address in Settings › Webhooks, press "Test event" and check the
signature as [above](#app-events-the-event-journal-and-webhooks); a `2xx` answer stops the retries.

**Python**, outside the app, through `api.json`:

```python
import json, os, socket
from pathlib import Path

base = os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config"
api = json.loads((Path(base) / "krad-ade" / "api.json").read_text())
host, port = api["endpoint"].rsplit(":", 1)

def envelope(request):
    line = {"api_version": 1, "token": api["token"], "from": "Client", "request": request}
    return (json.dumps(line) + "\n").encode()

def call(request):
    with socket.create_connection((host, int(port))) as sock:
        sock.sendall(envelope(request))
        return json.loads(sock.makefile().readline())

for agent in call("List")["Agents"]:
    print(agent["environment"], agent["terminal"], agent["state"], agent["title"])

with socket.create_connection((host, int(port))) as sock:
    sock.sendall(envelope({"SubscribeAgents": {}}))
    for line in sock.makefile():
        message = json.loads(line)
        if "AgentEvent" in message:
            event = message["AgentEvent"]
            print(event["seq"], event["terminal"], event["previous"], "->", event["state"])
        elif "Error" in message:
            raise SystemExit(message["Error"]["code"])
```

On Windows the file is in `%APPDATA%\krad-ade` unless `XDG_CONFIG_HOME` or `HOME` is set.

**Node.js:**

```js
const fs = require("fs");
const net = require("net");
const os = require("os");
const path = require("path");
const readline = require("readline");

const base = process.env.XDG_CONFIG_HOME || path.join(os.homedir(), ".config");
const api = JSON.parse(fs.readFileSync(path.join(base, "krad-ade", "api.json"), "utf8"));
const [host, port] = [api.endpoint.slice(0, api.endpoint.lastIndexOf(":")), Number(api.endpoint.split(":").pop())];

const socket = net.createConnection({ host, port }, () => {
  socket.write(JSON.stringify({ api_version: 1, token: api.token, from: "Client", request: { SubscribeAgents: {} } }) + "\n");
});
readline.createInterface({ input: socket }).on("line", (line) => {
  const message = JSON.parse(line);
  if (message.AgentSnapshot) console.log(`${message.AgentSnapshot.agents.length} agents`);
  if (message.AgentEvent) console.log(message.AgentEvent.terminal, message.AgentEvent.state);
  if (message.Error) socket.end();
});
```

## Compatibility and stability

**Stable in v1.** These keep their names, shapes and meaning for as long as the app serves v1:

- The transport: loopback TCP, one line per request, one line per answer, and the streaming of
  `SubscribeAgents`.
- The envelope (`api_version`, `token`, `from`, `request`), the `"Client"` and `Terminal` callers,
  and `Hello`.
- The requests and responses in [Agents](#agents), [Messages and screens](#messages-and-screens),
  [Workers](#workers), [Sessions, worktrees, files and changes](#sessions-worktrees-files-and-changes),
  [Kraddies and automations](#kraddies-and-automations), [Memory](#memory), and `RunVariants` and
  `PickVariant`, with the fields shown.
- The agent states, coarse states, event kinds and reasons; `seq`, `stream`, the snapshot and resume
  rules and the heartbeat.
- The error codes and the CLI's exit codes.
- The discovery file's fields and its `KRAD_ADE_*` variables.
- App event records with `api` 1, their kinds and fields, and the webhook headers and signature.

**How v1 changes.** Only by addition: new requests, new response kinds, new optional fields, new
error codes, states or reasons. The app ignores fields it does not know in a request, and clients
should do the same with answers, and treat an unknown response kind or code as a refusal they do not
understand. A change that would break a v1 client gets a new `api_version`; the app answers only the
version it serves, and refuses any other with `unsupported_api_version`.

**Not stable.** The text of messages and of the CLI's plain output (use `--json`), the display text
in `next_run`, `last_outcome`, `schedule` and `triggers`, the body of `Variants` answers, and the
requests listed under [Requests the app's integrations use](#requests-the-apps-integrations-use).
