---
status: accepted (amended by ADR-0059)
---

# Web clients reach runtimes over a WebSocket on the private network only

Krad Web is a full replica of the app's window in a browser, used by the one owner of the runtimes. It needs a transport a browser can open, and ADR-0054 left a `Listener`/`Stream` seam for it. A new `krad-web-listener` crate implements that seam over WebSocket, reusing the handshake, version check, permissions and codec of the Unix socket. Each WebSocket message carries one `krad-protocol` frame without its length prefix. The same listener serves the web client's compiled assets, embedded in the binary, so the page and the socket share an origin (no CORS, an `HttpOnly` `SameSite=Strict` cookie holds the token) and the web client's version is always the runtime's.

- **Reach.** The listener binds to loopback, or to an address of the user's tailnet (100.64.0.0/10, fd7a:115c:a1e0::/48). It refuses any other bind. Off loopback, TLS is mandatory and comes from a certificate the user provides, normally `tailscale cert` for `<machine>.<tailnet>.ts.net`. Without a certificate it only listens on loopback. There is no public mode: a runtime is never reachable from the internet, so the token is never the only barrier.
- **Pairing.** A browser pairs like any other client. An already paired client with `admin` makes a one-use code with `CreatePairingCode`, choosing its permissions (read, answer and drive proposed, admin only if ticked), shows it as a code, URL and QR, and the browser redeems it. The listener checks `Origin` and rate-limits redemptions.
- **One pairing per runtime.** The web client pairs separately with this computer's runtime and with each cloud runner, and aggregates them by `RuntimeId` as the app does. No runtime relays another's feed.
- **Web access** is a setting of each runtime, off by default: on, bind address, certificate and "Pair a browser" in the app's Settings.
- **Protocol types** for the web client are generated from the Rust types (`ts-rs` or `specta`), with a test that fails when they drift.

## Considered Options

- **Exposure left to a reverse proxy or Tailscale Serve, loopback only.** Simplest listener, but the owner wanted direct tailnet binds without a proxy in between.
- **A built-in `--tls` public mode.** Rejected: it makes the token the only barrier, and would need hardening, rate limiting and probably passkeys for a single user who already has a private network.
- **A central relay.** Rejected: one more service to run, and it sees every frame.
- **Plain HTTP on the tailnet.** Rejected: WireGuard encrypts the traffic, but the browser does not treat it as a secure context, so the clipboard API, service workers and `Secure` cookies are missing, and copy and paste in the terminal and editor would break.

## Consequences

- The web client and its build live in `rmd-ade/web`, in the same repository as the runtime, so types are generated and assets embedded in one build.
- Window-only features (traffic lights, Mica, Glass, window tint, dock badge, native menu) have no web counterpart.
