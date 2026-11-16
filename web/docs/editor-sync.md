# Editor sync

Editor buffers live in the runtime (ADR-0058, `../../docs/adr/0058-editor-buffers-live-in-the-runtime.md`).
The desktop and every browser edit the same buffer at once; operational transformation keeps them on
one text.

## The Rust reference

`../../crates/krad-domain/src/text_edit.rs` is the single definition of the algorithm:

- `TextEdit { start, end, text }` replaces the UTF-16 range `[start, end)` with `text`.
- `transformed_over(applied, order)` rewrites an edit so it applies after `applied`. `order` says
  which of the two the runtime placed first; it only breaks exact ties.
- `VersionedText` is the runtime's side; `EditQueue` is a client's side.

## Port it, do not redesign it

Port `TextEdit`, `transformed_over` (with `map_start`, `map_end`, `covers`, `shift`, `after_text`),
`snapped_to`, `applied_to` and `EditQueue` to `src/editor/ot.ts` line by line, keeping every rule
and tie-break. Port the Rust tests too, including the exhaustive pair test over the three texts
(ASCII, accented, emoji) and the random two-client simulation; they are the proof the port matches.
Done when those tests pass in Vitest with the same inputs.

## The client protocol

1. `editor/open { environment, path }` → a `buffer_opened` event with `buffer`, `version`, `text`.
   Opening a file that is already open answers with the same buffer.
2. Local typing: apply the edit to the editor at once and `queue.push(edit)`.
3. Send with `queue.takeNext()`: at most one edit is in flight. Send
   `editor/edit { buffer, based_on, edit }` and remember the request id.
4. On `buffer_edited`: if `author` is your in-flight request id, call `queue.acknowledged()` and send
   the next one; otherwise `const incoming = queue.heard(edit)` and apply `incoming` to the editor.
5. A `refused` reply with code `invalid` means the base is too old: reopen the buffer and replace
   the local text with the snapshot's.
6. `editor/save` writes to disk (`buffer_saved`); `editor/close` closes it for every client.

Language features (`language` area) take the buffer id and a `{ line, character }` position:
completion, hover, definition, references, signature help, rename, format, and `install_server`
when a `language_server` event says `not_installed` (ask the user first). Diagnostics arrive as
`diagnostics` events per file.
