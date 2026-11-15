---
status: accepted
---

# Editor buffers live in the runtime, edited with versioned, rebased edits

Buffers, undo and the language servers live in `krad-ui` with `krad-lsp`. With the app's window and a web client driving the same runtime, two clients can open the same file. The buffer moves into the runtime, so there is one copy per file and one language server per folder, and both clients see each other's edits live. New protocol areas `editor` (open, edit, save, close, undo) and `lsp` (diagnostics, completion, hover, definition, rename) carry it. The app's editor becomes a client of them too.

- **Edits.** Each edit names the buffer version it was made on. The client applies it at once, so typing never waits on the network. The runtime transforms it over the edits that arrived first (plain-text operational transform) and broadcasts the result with the new version. The client rebases its pending edits on what it receives.
- **Tabs and panes.** Which tabs a session holds (chats, terminals, files) is runtime state shared by every client. How a client splits them into panes, their sizes and which one has focus belong to that client.
- **Preferences.** The theme and appearance settings live in the runtime and are shared. Layout preferences stay with each client.

## Considered Options

- **Buffers per client.** The runtime would only serve files, change events and per-document LSP, with less migration on the desktop. Rejected: two copies of the same file, and a "changed on disk" conflict whenever both clients save.
- **One writer at a time.** No transform needed, but the other client turns read-only while you type, which defeats live use from two places.
- **A CRDT (Yjs, Automerge).** Proven merging, but it is a large dependency on both sides and a data model unlike the current rope.

## Consequences

- This is the largest change Krad Web asks of the runtime, and it changes the desktop's editor too. It comes after the web client is already useful for chat, terminals and git.
