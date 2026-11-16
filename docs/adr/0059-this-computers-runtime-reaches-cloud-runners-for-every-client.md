---
status: accepted
---

# This computer's runtime reaches the cloud runners for every client

ADR-0056 had each web client pair with every runtime separately and ruled out a relay, and the app's window opened its own line to each cloud runner. The owner wants one web address, reached from a phone, where a session works exactly as in the app whether its chats run on this computer or on a cloud runner: a cloud chat is just a chat of a session. So this computer's runtime opens the lines to the cloud runners itself and serves their sessions to every client, the app's window and web clients alike; there is one path, and no client talks to a runner directly. This amends ADR-0056.

- **Web access is turned on from the app.** One switch in Settings runs this computer's runtime as a service (ADR-0057), serves web access on the tailnet with the certificate from `tailscale cert`, and shows a QR code and a one-use code to pair a browser.
- **Phone first.** The web client keeps every function on a phone with a layout of its own, and looks like the app's window on a large screen.
- **Notifications.** Installed as a PWA, the web client receives Web Push notices when an agent waits for an answer or finishes, through the browser's push service.
- **Awake.** While web access is on, the computer stays awake while an agent works, and for up to two hours while one waits for an answer.

## Consequences

- The runtime takes over the runner lines and the cloud session views the window holds today; the window becomes a client of them like the web.
- With this computer asleep or off, nothing is reachable, cloud sessions included.
