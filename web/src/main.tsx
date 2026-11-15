import { StrictMode, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { RuntimeClient, type ConnectionStatus } from "./runtime/client";
import { redeemPairingCode } from "./runtime/http";
import { applyFeed, EMPTY, reduceEvent, type RuntimeState } from "./runtime/store";

const BUILD = "krad-web-0.4.1";

function App() {
  const client = useMemo(() => new RuntimeClient({ build: BUILD }), []);
  const [status, setStatus] = useState<ConnectionStatus>({ status: "connecting" });
  const [state, setState] = useState<RuntimeState>(EMPTY);
  const [code, setCode] = useState("");
  const [problem, setProblem] = useState<string>();

  useEffect(() => {
    const stopStatus = client.onStatus(setStatus);
    const stopFeed = client.onFeed((feed) => setState((now) => applyFeed(now, feed, reduceEvent)));
    client.connect();
    return () => {
      stopStatus();
      stopFeed();
      client.close();
    };
  }, [client]);

  const paired = status.status === "open" && (status.welcome.permissions?.length ?? 0) > 0;

  async function pair() {
    try {
      await redeemPairingCode(code, navigator.userAgent.slice(0, 60) || "Browser");
      location.reload();
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <main style={{ padding: "var(--krad-space-6)", color: "var(--krad-text-strong)", background: "var(--krad-bg-surface)", minHeight: "100vh", fontFamily: "Inter, system-ui, sans-serif" }}>
      <h1 style={{ fontSize: "var(--krad-text-lg)" }}>Krad Web</h1>
      <p>Connection: {status.status}</p>
      {!paired && (
        <form onSubmit={(event) => { event.preventDefault(); void pair(); }}>
          <input value={code} onChange={(event) => setCode(event.target.value)} placeholder="Pairing code" maxLength={8} />
          <button type="submit">Pair</button>
          {problem && <p style={{ color: "var(--krad-error)" }}>{problem}</p>}
        </form>
      )}
      {paired && <p>Environments: {state.snapshot?.environments.map((environment) => environment.name).join(", ") || "none"}</p>}
    </main>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
