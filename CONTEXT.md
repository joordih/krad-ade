# Krad ADE — Context

Desktop ADE to run many coding agents at once, as chats or terminals, organised in sessions.

## Language

### Vision and product

**ADE**:
Agentic development environment to drive many terminals at once.
_Avoid_: IDE, terminal

**Work mode**:
How the whole window is laid out for the work at hand: ADE mode or Editor mode. Switching it closes nothing and loses no session, tab or open file.
_Avoid_: layout, view, perspective

**ADE mode**:
The default work mode: sessions and their chats and terminals take the work area, with the views Sessions, Notes and Review.
_Avoid_: agent mode, chat mode

**Editor mode**:
The work mode for editing code: open files take the work area, an agent's chat sits in the side panel and terminals in the utilities panel, the same ones ADE mode shows.
_Avoid_: IDE, code mode, Zed mode

**Language server**:
Program that understands one language of a group's files and gives their file tabs diagnostics, completion, hover, go to definition, references, rename, formatting and quick fixes. One runs per project folder, language and toolchain on the environment's host; the app installs it after asking, the first time a file of that language opens.
_Avoid_: LSP, plugin, extension

**Problem**:
An error, warning, info or hint a language server reports about a place in one file of a group, whether or not the file is open.
_Avoid_: diagnostic, issue, lint

**Problem lens**:
The message of a line's most severe problem, written after the end of that line in its file tab.
_Avoid_: error lens, inline diagnostic, virtual text

**Project check**:
The group's installed language servers looking at all of its files, not only the open ones, so their problems show in Problems and the file tree. It skips what git ignores and stops at a limit of files, saying the check is incomplete.
_Avoid_: index, indexing, scan, analysis

**File template**:
A kind of file the user can create in a folder of the file tree, such as a Java class, interface or package, or a React component, offered by the language server serving that folder or, when it offers none, by the language's manifest.
_Avoid_: new item, scaffold, snippet

**Minimap**:
A narrow, scaled-down picture of a file tab's whole text beside it, showing where the visible part sits; dragging or clicking it scrolls the file tab.
_Avoid_: overview, code map

**Language manifest**:
Data that defines a language for the app: its file extensions, comment marker and highlighter grammar, its default language server from the package registry, how that server starts, which server runtime it needs and how the project's toolchains are found and handed to it. Adding a language is writing one, never code.
_Avoid_: language config, language plugin

**Toolchain**:
The interpreter, SDK or version a project's code runs with, such as a Python virtual environment, a conda environment, the project's own TypeScript, a Node.js version or a JDK. The app finds the candidates on the environment's host and the user picks one per project folder of a group; the editor's language servers analyse the code with it, and the user may have it activated in the group's new terminals. The app never installs one.
_Avoid_: interpreter, SDK, environment, runtime

**Project folder**:
A folder inside a group that holds its own project, marked by a file such as `pyproject.toml` or `package.json`; a file belongs to the nearest one, and the group's root is the default.
_Avoid_: subproject, workspace, package, module

**Server runtime**:
The program a language server itself runs on, such as Node.js for a TypeScript server. It is found on the host, or the app installs a private copy in its own folder; it is never the toolchain the project's code is analysed with.
_Avoid_: toolchain, runtime (alone), dependency

**Package registry**:
Mason's public list of language servers and tools and how to fetch each one, which the app downloads and caches.
_Avoid_: Mason (in UI), catalogue, marketplace

**Extension**:
A folder the user adds from Settings, from a git address, the extension index or a local folder, and can update, disable or remove. It brings language, toolchain and server runtime manifests, and may bring its own program, which adds commands, views drawn by the app, host kinds and tools for agents. Nothing in it is used until the user trusts it, and any change to its files or its commit asks again.
_Avoid_: plugin, add-on, Zed extension, VS Code extension

**Extension program**:
The program an extension brings, which the app starts when one of its commands, views, host kinds or tools is first used. It runs either once for the app on this computer or once per open environment on that environment's host, as its extension declares. It may open terminals or run other extensions' commands only when its extension declares those permissions.
_Avoid_: extension host, plugin process, extension server

**Extension view**:
A view an extension describes and the app draws, from its own building blocks and ready-made trees, tables, forms and terminals in the theme's colors, in the sidebar, side panel, utilities panel or a file tab as the extension names. It stays closed until the user opens it, and nothing in it can make the app run code.
_Avoid_: webview, panel, extension UI

**Host kind**:
A kind of machine an environment can live on besides this computer and SSH hosts, added by an extension, such as a Docker container. Its extension lists the machines and opens a channel to one; the app does everything else there as on an SSH host.
_Avoid_: provider, remote type, connector

**Extension tool**:
A tool a trusted extension offers to every agent the app starts, beside the code tools, named after its extension. It is never presented as read-only, so each agent's permission mode asks before using it unless the user allows it there.
_Avoid_: plugin tool, extension command

**Environment**:
Work context the user handles as one: its groups, which may come from different repositories, and their sessions. It lives on one host, and everything inside it runs there.
_Avoid_: project, profile, workspace

**Host**:
The machine an environment lives on: this computer, a remote machine reached over SSH and named by its SSH destination, or a machine of a host kind an extension adds, such as a Docker container. Its groups' folders, terminals, chats and agents are all on it.
_Avoid_: server, remote, machine, connection

**Tab**:
One entry of a pane's strip of tabs: a chat, a shell, an agent or a file tab.
_Avoid_: workspace, group, desktop

**Tab picker**:
Searchable list of a strip's tabs opened from the button at its start, with each tab's shortcut; typing a name no tab has creates one.
_Avoid_: overflow menu, tab switcher, all tabs

**Navigation history**:
The places in files the user jumped between in a session (a definition, a reference, a problem, a search result, a newly opened file), walked back and forward with a two-finger swipe, the mouse's back and forward buttons or their shortcuts.
_Avoid_: jump list, breadcrumbs, undo

**Session**:
A line of work the user opens inside a group, holding its own tabs; it opens on a chat. Every group keeps at least one, and removing a group removes its sessions.
_Avoid_: workspace, thread, PTY session

**Cloud session**:
A session that runs on a cloud runner instead of the environment's host, so its chats, workers and terminals keep working while the app is closed. It works in its own clone of the group's repository, on its own branch, and the user opens it from any group of any environment or sends an existing session there. A question or permission it raises while nobody is connected waits for the user.
_Avoid_: background agent, remote session, cloud agent

**Cloud runner**:
A Linux machine of the user's where the app installed the `cloud-runner` daemon of its host CLI as a user service, reached through the system `ssh`, which runs cloud sessions and keeps them alive without the app. Every installation of the app paired with it sees all its cloud sessions. It carries the agent configuration the user chose to sync, and its harnesses and git sign in on the runner itself, started from the app.
_Avoid_: runner, cloud, server, worker, remote host

**Pairing**:
Giving one installation of the app a revocable token for a cloud runner, created over SSH on the runner. The daemon only listens on the runner's loopback, so the token is never the only barrier.
_Avoid_: login, linking, registration

**Archive**:
The closed sessions of a group with their chats, and the chats closed from sessions still open, kept so the user can look through them and reopen them, until the user deletes them or the archive retention removes them. A reopened session gets its chats and file tabs back; its shells start afresh. A lead's session is archived and reopened with its Workers; removing a group deletes its Archive. A session with nothing worth reopening (no used chat, pinned file or Worker) is not kept.
_Avoid_: history, trash, recently closed

**Archive retention**:
How long the Archive keeps what it holds before deleting it: always, a number of days, or not at all. Deleting removes what the app keeps of a chat, never the harness's own files.
_Avoid_: cleanup, expiry, TTL

**Session look**:
A session's own icon and the colour it is drawn in before the session's name in the sidebar, both picked by the user from the session's right-click popover, where it is also renamed.
_Avoid_: theme, style, label colour

**Window tint**:
The colour and opacity laid over the desktop behind a translucent window, which the user can pick on macOS; it always stays translucent.
_Avoid_: background, wallpaper

**Backdrop image**:
A picture the user picks to paint behind the whole window, under its panes and over the window material, with its own fit, opacity, blur and dim; it is copied into the config folder, shared by the light and dark themes, and hidden while the system reduces transparency.
_Avoid_: wallpaper, background image

**Pane**:
One part of a session's view, with its own strip of tabs and one of them on screen; a session shows up to four.
_Avoid_: split, tile, window, frame

**Snap zone**:
A half or a quarter of a pane that a dragged tab can be dropped into, which splits the pane.
_Avoid_: drop target, dock area

**Utilities panel**:
The panel along the foot of the work area, opened from its arrow: tabs for the activity log, the system terminals and the terminals dropped on it.
_Avoid_: activity panel, bottom panel, console

**Layout template**:
A ready arrangement of a session's panes (two columns, two rows, main and stack, two by two), offered at the top centre of the view while a tab is dragged.
_Avoid_: preset, arrangement, snap layout

**Group**:
Set of an environment's terminals bound to a root folder on the environment's host: they start there, and the file tree and git state come from there.
_Avoid_: project, folder, workspace, tab

**Worktree**:
Separate git checkout of the group's repository on its own branch, opened as a new group so an agent can work without touching other checkouts.
_Avoid_: clone, copy, sandbox

**File tree**:
Folders and files under the selected group's root, with their git changes and problems; the user can create, rename, move and delete them there. It sits in the side panel and can climb above the group's root to any folder of the disk, showing git changes only inside a repository.
_Avoid_: explorer, navigator, browser

**Side panel**:
The panel on the right of the window with the file tree and the group's git changes.
_Avoid_: explorer, inspector, right sidebar

**Discard**:
Throwing away a file's uncommitted changes so it matches the last commit again; a file git does not track yet is deleted. A single block of changed lines can be reverted the same way from its diff.
_Avoid_: reset, undo, rollback

**Diff comment**:
A comment the user leaves on a line or range of lines of a file's Changes diff for the session's agent, kept with the session until sent. It follows its hunk while line numbers shift and shows as outdated once that hunk changes. "Send review" sends every comment of the session to its agent as one message and clears them.
_Avoid_: review comment, note, annotation, discussion

**History**:
The commits of the group's branch and its upstream, newest first, drawn as a graph of lanes under the git changes. A commit opens to list the files it changed, each shown as a diff against its first parent.
_Avoid_: log, timeline, graph

**Branch switcher**:
The searchable list of the group's worktrees, local branches and remote branches, opened from the environment pill or the branch in the git changes. Picking a worktree opens its group; picking a branch checks it out in the selected group, a remote one through a new local branch that tracks it.
_Avoid_: worktree window, branch picker, checkout dialog

**Staged change**:
A change the user has marked to go into the next commit; a commit with nothing staged takes every change.
_Avoid_: index, cached change

**Stash**:
Uncommitted changes the user set aside to clean the working tree, to bring back later.
_Avoid_: shelf, saved changes

**File tab**:
A tab of the active session that edits one file with syntax highlighting. One click on a file opens it as the session's preview tab; a double click on the file or on the tab pins it. Pinned file tabs reopen with the session; the preview tab does not.
_Avoid_: editor window, buffer, document

**Preview tab**:
The one file tab of a session that is not pinned, shown in italics; opening another file with one click replaces it.
_Avoid_: temporary tab, transient tab

**Notes**:
The group's notes and documentation as Markdown files in a vault inside its repository, written by hand or generated by an agent; it replaces the earlier Wiki.
_Avoid_: wiki, docs site, knowledge base

**Vault**:
The folder of a group's repository that holds its notes, with their folders and attachments; one per group.
_Avoid_: workspace, notebook, library

**Docs source**:
Folder of the group the user picks so its existing documents feed the notes an agent generates; the agent checks them against the code and writes notes that cite them, leaving the documents themselves untouched.
_Avoid_: wiki source, note source, import folder, scan path

**Note**:
One Markdown file of the vault, written by hand as Markdown; a note an agent wrote records which model wrote it from which sources.
_Avoid_: page, wiki page, document

**Attachment**:
An image, video or other file a note embeds, kept in the vault's attachments folder.
_Avoid_: asset, media, upload

**Backlink**:
A note that links to the one open, seen from the note it points to.
_Avoid_: reference, inbound link, mention

**Code host**:
GitLab or GitHub, where a group's git remote lives and its merge requests are reviewed; the user signs in to each one separately.
_Avoid_: forge, provider, git server

**Review**:
View of the group's merge requests on its code host, with their commits, changes, discussions and pipelines, where the user comments, approves and merges. It is offered only while the group's remote is on a code host the user is signed in to.
_Avoid_: GitLab view, MR panel, code review

**Merge request**:
Proposal on a code host to merge a branch, reviewed by other people. GitHub calls it a pull request, and the app uses that name for groups on GitHub.
_Avoid_: MR, PR (in prose)

**App update**:
Newer stable Krad ADE release that an installed app finds on GitHub, downloads, verifies and installs over itself, so the user never reinstalls by hand. It is available, then downloading, then ready: a ready update waits for the user to restart the app, or applies when the app quits. When the app cannot replace itself, the update is only announced.
_Avoid_: upgrade, self-update, release (for the thing the user gets), patch

### Spatial unit

**Terminal**:
Smallest unit of an environment: a terminal view bound to a single shell process.
_Avoid_: console, session, agent, window, panel

**System terminal**:
A terminal of the environment that belongs to no group or session and starts in the user's home folder on the environment's host; it lives in the utilities panel and reopens with the environment, with a new shell.
_Avoid_: global terminal, scratch terminal, project terminal

### Processes and agents

**Process run**:
Life of the shell process bound to a terminal, from start to exit.
_Avoid_: PTY session, terminal session, connection, job

**Resume id**:
Identifier a harness gives its conversation so a later process run can continue it with its history.
_Avoid_: session id, conversation id

**Agent**:
Role a terminal takes when it runs an agentic CLI such as Claude Code, opencode or Codex.
_Avoid_: assistant, bot, agent entity

**Harness**:
Agent CLI the app knows how to detect on `PATH` and start in a terminal, such as Claude Code, Codex or opencode.
_Avoid_: provider, integration, plugin

**Twin**:
The ACP agent that stands for a native harness, such as Claude Code's ACP adapter for Claude Code. Both carry the harness's name and mark; pickers offer only one of the two, the ACP one while it can run, and chats keep the one they started with.
_Avoid_: duplicate, alias

**Model**:
Language model a harness runs, such as `sonnet` for Claude Code; Settings keeps the list each harness offers.
_Avoid_: engine, LLM, provider

**Gateway**:
Server the user runs that offers many models from their accounts behind one Anthropic- and OpenAI-compatible address and key, such as CLIProxyAPI. The app knows at most one, and offers it as a harness of its own for chats: the app's own agent works with the Gateway's models.
_Avoid_: proxy, provider, backend, router

**MCP server**:
Program or address that offers tools to the Gateway's agent over the Model Context Protocol. It comes from Settings or from a project's `.mcp.json`, which the user trusts or ignores once per folder; its tools run under the chat's permission mode like any other.
_Avoid_: plugin, extension, connector, integration

**Chat**:
Conversation with a harness and model rendered natively instead of as a terminal, kept by the harness under its resume id and restored with its history when the app reopens; messages sent while it works wait in a queue. An agent's chat surface, the lead's included, is a different thing: a view of that agent's own conversation (see Surface). With Claude Code it can also orchestrate: start Workers in the group through the Agents bus, type instructions into them and check their work.
_Avoid_: assistant, agent, conversation, agents bus

**Mention**:
A file, folder or image sent with a Chat message as context: typed as `@path`, picked, dropped or pasted. Images reach the model as images and other files by their path; pasted images are kept with the chat and go with it when it is deleted.
_Avoid_: reference, link, tag, attachment, upload

**Chat search**:
Finding words across the history of every chat of the selected group from the command palette, each hit showing the chat and a snippet of the message, from where the user can reopen the chat, fork it or export it to a note.
_Avoid_: history search, global search, find in chats

**Fork**:
A new chat that continues another one from one of its messages, leaving the original as it was: through the harness's own fork when it can, otherwise seeded with the conversation up to that message.
_Avoid_: branch, copy, clone, duplicate

**Rewind**:
Cutting a Chat back to one of the user's messages, edited, so the harness answers it again; it rewinds the conversation, and the files too where the harness can restore them or a checkpoint of that turn exists. Only while the chat is idle.
_Avoid_: edit, retry, regenerate, undo

**Checkpoint**:
The state of a Group's git repository's files saved by the app just before a Chat turn, for harnesses that cannot restore files themselves, so a rewind can put them back; restoring one first saves the current files as another, so the restore can be undone.
_Avoid_: snapshot, backup, stash

**Checklist**:
The steps an agent tracks for its current work, kept up to date as it goes and shown above a Chat's composer.
_Avoid_: todo list, plan, tasks

**Ask**:
A tool call a chat waits on the user for: a permission to run a tool, questions to answer by picking options or writing one's own, or the approval of a plan. While it waits it takes the place of the composer.
_Avoid_: prompt, approval, confirmation, dialog

**Permission mode**:
How freely a chat may run tools without an Ask: manual, accept edits, plan or auto. It starts as Claude Code's own setting and keeps the last one the user picks for that chat.
_Avoid_: approval mode, safety level

**Hook**:
Report an agent CLI sends to the app at a point of its work (a prompt submitted, a tool run, a permission asked, a turn finished); it sets the agent state.
_Avoid_: callback, event, webhook

**Worker**:
Agent a lead starts through the bus to do part of the work, briefed to report back when done. It opens as a session of its own in the group, as a chat, without taking the user's place on screen; the lead owns it and its chat shows it as a Worker card. It closes when it fails to start, once it has finished and the lead has read its work, when the lead closes it (its chat is kept and reopens from the card, its worktree stays on disk unless the lead asks to remove a clean one), and when its lead is removed.
_Avoid_: sub-agent, child, job, terminal

**Kraddie**:
The creature species of the app, and the feature named after it (the Kraddies): a Worker with an identity that lives between sessions, with a name the user gives it (there is no default name), a painted design, a harness and model and the repository it works in. It owns automations, its routines, and each of their runs works as a Worker session without a lead, reporting through the attention queue. Kept as a `.kraddie` file in the user's config folder (older `.kradpet` files and the folder of its former name still load), with its automations next to it. Its base art is also called Kraddie.
_Avoid_: bot, cron job, mascot

**Automation**:
One routine of a Kraddie: a name, an active switch, triggers (a schedule, a GitHub or GitLab event, an app event of the Kraddie's repository, or a manual run from the app or `krad automation run`), instructions, optional harness, model, effort and permission mode, where its runs work and which conversation they continue, and a catch-up window for runs missed while the app was closed. Made from scratch or from an example in the Automations view; counted on the Kraddie page as its routines.
_Avoid_: workflow, job, rule, habit

**Kraddie run**:
One run of a Kraddie's automation: when it started, what woke it, its outcome and the session it opened, kept in the automation's bounded history.
_Avoid_: job, execution, task

**Kraddie page**:
A Kraddie's own page in the Kraddies view: a stage with its animated look and mood, and five parts picked from a sliding switch: At a glance (its Trail, Day so far, On the horizon, Fresh from today and Logbook cards), Compass, Keepsakes, Chats and memory (its Kraddie chats, its chat digests and its memory reach) and Tuning (look, brain, reach, its sessions, rounds and a fresh start). Cards reflow with the width of the content island.
_Avoid_: profile screen, dashboard

**Trail**:
The heat map of a Kraddie's last six months, one square a day, by runs, lines, tokens or cost.
_Avoid_: contributions, activity graph

**Compass**:
The guidance a Kraddie carries on every run, a Markdown document kept as `compass.md` in its config folder (a file of its former name is renamed once).
_Avoid_: system prompt, persona, soul

**Keepsakes**:
The bounded list of short notes a Kraddie carries between runs (its memory in code and in `krad kraddie memory`); the user edits them on the Kraddie page and the Kraddie adds one with `krad kraddie remember`.
_Avoid_: notes, knowledge base

**Kraddie chat**:
A chat session that belongs to a Kraddie, opened from its row in the sidebar, its page or `krad kraddie chat new`: it starts with the Kraddie's harness, model, effort and permissions, its identity, Compass and Keepsakes, works in the Kraddie's working folder when it has one (otherwise in the group the user is in), carries the Kraddie's avatar in the session lists and is reopened, renamed and closed like any session.
_Avoid_: Kraddie session, persona chat

**Chat digest**:
The short list of facts and decisions a chat leaves behind, written in the background by the chat's own harness when the chat ends or rests for a while, with secrets redacted; kept per Kraddie for Kraddie chats and in one shared book for ordinary chats, and read, edited or forgotten by the user. A Kraddie can turn its automatic digests off.
_Avoid_: summary, transcript, log

**Memory reach**:
Which memory a chat may read: the digests of ordinary chats always, plus, for a Kraddie chat, its own Kraddie's Keepsakes and digests and those of every Kraddie it can read memory of (one way, set on the reader, not passed on), plus any Kraddie the user mentions with `$` in that chat. Applied to the memory pack and to every `memory_search` and `memory_read` call.
_Avoid_: permissions, sharing scope

**Memory pack**:
The memory within a chat's reach ranked by relevance and recency and cut to a token budget, handed to the harness when the chat starts; a `$` mention attaches the named Kraddie's own pack to the message.
_Avoid_: context dump, RAG

**Rounds**:
A Kraddie's optional read-only look over its work after a quiet spell of a chosen span; it only suggests what to do next, through the attention queue.
_Avoid_: ping, keepalive, auto-run, heartbeat

**Kraddie studio**:
The sheet, opened from the Kraddies view or the Kraddie page, where the user paints a Kraddie's design on layers and faces with brush, eraser, fill and shapes, a palette, and undo and redo.
_Avoid_: editor, designer, paint tool

**Worker card**:
The preview of one Worker inside its lead's chat: its task, agent state and latest activity, kept live; clicking it goes to the Worker's session.
_Avoid_: subagent card, worker preview, worker tab

**Variant run**:
The same task handed to two to four Variants at once, owned by the group it started from, so the user can compare them side by side in the Variants view and pick a winner; the others are discarded with their worktrees.
_Avoid_: experiment, race, A/B test

**Variant**:
One attempt of a Variant run: a harness, an optional model and an optional extra instruction, working in a worktree of its own as a Worker session the run owns. Its diff, test result, cost, tokens and time are recorded for the comparison.
_Avoid_: candidate, branch, fork

**Subagent**:
Helper a harness starts inside its own turn, such as one Claude Code or the Gateway's agent runs through its agent tool, with a fresh context; it has no terminal and the bus does not know it. The agent's chat shows it as a card the user can go into to read its whole conversation.
_Avoid_: worker, child agent

**Lead**:
The chat of a session the user turns into its orchestrator from the composer: only it starts and directs Workers through the agents bus.
_Avoid_: orchestrator, master, coordinator

**Surface**:
How an agent's conversation is shown: as its own CLI interface in a terminal, or as a chat. A chat is either terminal-backed, a view over the running terminal that switches in place, or structured, a tab driving the harness without a terminal; each can continue a copy of the other's conversation in a new tab.
_Avoid_: view, mode, UI

**Agents bus**:
Channel through which agents in the app's terminals, and the Chat, list each other, exchange messages and wait for each other while the app runs.
_Avoid_: orchestrator, chat, MCP

**Agents API**:
The versioned protocol of the Agents bus, spoken by the `krad` CLI: each request carries its API version and every refusal a stable error code. Besides coordinating agents it lets a lead close its Workers, lists, creates and closes sessions, creates worktrees, opens files and reads a group's changes.
_Avoid_: public API, REST API, SDK

**Agent event**:
One line of an Agents API subscription: an agent opened, changed state or closed, numbered within the run of the app, with its previous and new state and the reason when known. A subscription starts with a snapshot of the open agents and stays alive with heartbeats.
_Avoid_: app event, notification, webhook

**API discovery file**:
The owner-only `api.json` the app leaves in its config folder while it runs, naming the Agents API's endpoint, token, version and process, so programs outside the app's terminals can reach it; Settings can turn it off.
_Avoid_: lock file, socket file, config

**App event**:
A typed fact the app publishes when its own state changes: an agent starts, waits for input or finishes, a worktree or session opens or closes, a group's git changes, a review is sent, or a merge request opens or its pipeline passes or fails. It names ids, never contents, carries the version of its format and reaches the group's event journal, the webhooks that chose it and the extensions that subscribed to it.
_Avoid_: hook, notification, signal, message

**Event journal**:
The app events of one group kept on disk as lines in a bounded file that rotates, read back skipping any line it cannot read.
_Avoid_: log, history, audit trail

**Webhook**:
An address the user adds in Settings that the app posts the app events it chose to, signed with its secret when it has one and retried a few times when it fails. Events from the code host reach the app by asking it now and then, never through an address the app opens.
_Avoid_: hook, callback, integration, sink

**Agent state**:
Observable situation of an agent: idle, working, waiting-for-input or done (finished its turn and not yet seen).
_Avoid_: status, presence, activity

**Attention queue**:
The agents waiting for the user, across environments: permissions first, then asks, errors and finished turns the user has not seen, oldest first within each. It feeds the app icon's count, the system notifications shown while the window is in the background and the shortcut that jumps to the next waiting agent.
_Avoid_: unread, alerts

**Attention inbox**:
The panel, closed until opened, listing the attention queue and the recent history of events, filtered by kind: finished, asks, permissions and errors.
_Avoid_: notification center, feed

**Quiet hours**:
The daily span in which no notification or sound is sent; the attention queue, the inbox and the count still fill. A muted session is quiet all day.
_Avoid_: do not disturb, snooze

**Theme**:
The colors of the whole app in one set: the interface, the terminals' ANSI colors and the code editor's syntax. The user picks one theme for light appearance and one for dark, and the system appearance can choose between them. It does not change how translucent the window is.
_Avoid_: skin, color scheme, terminal theme

**Theme library**:
The themes the user can pick from: the app's own, well-known editor themes shipped with the app under their authors' licenses and credited to them, and the user's themes made from one of those in the app.
_Avoid_: theme store, marketplace, gallery

### Interaction

**Focus**:
The single terminal that receives the keyboard at any moment.
_Avoid_: active focus, selection, cursor

**Command palette**:
Filterable list to run app commands from the keyboard.
_Avoid_: command-K, launcher, global search

**Project search**:
Search across the files of the group's folder from the side panel, as text or a regular expression, matching case or whole words, narrowed by files to include and exclude; its replace shows each change before it and applies to one file or to all of them.
_Avoid_: global search, grep, find in project

**Project task**:
A command the group's folder already declares for building, testing or running it: a `package.json` script, a Cargo command, a `justfile` recipe or a `Makefile` target. The command palette runs one in a new terminal in the group.
_Avoid_: job, script, checklist

**Settings**:
Persistent user preferences: appearance, window material and tint, terminal font and theme, and integrations.
_Avoid_: preferences, config, options

**Chrome**:
Fixed interface around the views: sidebar, top bar and auxiliary panels.
_Avoid_: UI shell, frame, decoration

**Top bar**:
Row above the work area with the tabs of a single-pane session or the open notes, and the side panel toggle.
_Avoid_: header, toolbar, title bar

**Toast**:
Short message the app shows over the window, of one kind (error, warning, success or info), optionally with one action such as updating a harness. Every toast lasts the same time the user sets and waits while the pointer is over it; what belongs to the attention queue stays in the attention inbox after it goes.
_Avoid_: notification, alert, snackbar, banner

**Toast placement**:
Where toasts appear in the window, chosen by dragging a sample toast in placement mode, snapping to the window's edges, its centre lines, the sidebar, the top bar and a grid. Placed in the left half, a toast carries its icon on the right and stacks from the left; in the right half, the other way round.
_Avoid_: toast position, corner, anchor

**Beta**:
A switch in Settings that turns on a redesigned part of the app beside the current one, such as the new composer, keeping every feature of the current one; the current one stays the default until the redesign is promoted.
_Avoid_: experiment, feature flag, labs

**Arcade**:
Dot-matrix banner above a new Chat's empty view, gone once the conversation starts. Each new Chat draws one Arcade game at random, never the one shown last; the banner's next-game control cycles to another. One Arcade serves the whole app, so every empty Chat on screen shows the same game.
_Avoid_: minigame, easter egg, game panel

**Arcade game**:
One of the Arcade's games: byte run, bug hunt, line break or token trail. It plays itself until the user takes control, a run ends at the first loss, and each game keeps its own best score across restarts.
_Avoid_: minigame, level, mode
