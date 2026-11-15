---
status: accepted
---

# This computer's runtime runs as a user service

ADR-0004 kept the MVP without a session daemon: closing the app ends its processes. Krad Web has to drive this computer while the app is closed, so on macOS and Linux this computer's whole runtime (environments, SSH hosts, Kraddies, automations, variants and the clients of cloud runners) moves into a user service built from the same headless runtime host as the cloud runner (ADR-0054). The app's window becomes a client of that service, like the web client. This supersedes ADR-0004 on macOS and Linux.

- **Lifetime.** The service is a launchd agent on macOS and a `systemd --user` unit on Linux. It starts at login and outlives the app's window. The user can turn it off in Settings.
- **Updates.** When the app updates, the service restarts on the new version once no agent is at work, or after asking the user. Harnesses are adopted again from their journal as on the cloud runner, and shells are relaunched as today.
- **Windows** keeps the runtime inside the app and has no web access for now. The service and orphaned ConPTYs that ADR-0004 avoided are still not worth it there.
- **One runtime per machine.** No part of the runtime stays in the app on macOS and Linux, so there are never two runtimes on the same computer.

## Considered Options

- **The app exposes its in-process runtime.** Fastest to build, but it only works while the app is open, and pairing and permissions, which live in `krad-runner`, would have to move into the desktop process anyway. It would be built and then thrown away.
- **Only local sessions in the service.** Rejected: two runtimes on one computer, and the web client would not see SSH hosts.
- **Started by the app and kept alive.** Rejected: after a reboot the web client would not work until the app was opened.

## Consequences

- The warning about closing the app with agents at work goes away on macOS and Linux.
- The app's window and the runtime can run different versions for a while. The protocol is additive (ADR-0052), and the web client always matches the service because the service embeds it.
