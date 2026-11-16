import { useEffect, useState } from "react";
import { redeemPairingCode } from "../runtime/http";

const PAIR_FRAGMENT = "#pair=";
const DEVICE_LENGTH = 60;

function deviceName(): string {
  const agent = navigator.userAgent;
  const phone = /iPhone|Android/.exec(agent)?.[0];
  return (phone ? `${phone} browser` : agent).slice(0, DEVICE_LENGTH) || "Browser";
}

function codeFromAddress(): string | undefined {
  return location.hash.startsWith(PAIR_FRAGMENT) ? location.hash.slice(PAIR_FRAGMENT.length) : undefined;
}

export function Pairing({ refused }: { refused: string | undefined }) {
  const [code, setCode] = useState(codeFromAddress() ?? "");
  const [problem, setProblem] = useState<string>();
  const [busy, setBusy] = useState(false);

  async function pair(entered: string) {
    setBusy(true);
    try {
      await redeemPairingCode(entered, deviceName());
      history.replaceState(null, "", location.pathname);
      location.reload();
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error));
      setBusy(false);
    }
  }

  useEffect(() => {
    const linked = codeFromAddress();
    if (linked) void pair(linked);
  }, []);

  return (
    <form
      className="pairing"
      onSubmit={(event) => {
        event.preventDefault();
        void pair(code);
      }}
    >
      <strong style={{ color: "var(--krad-text-strong)", fontSize: "var(--krad-text-lg)" }}>Pair this browser</strong>
      <span style={{ color: "var(--krad-text-muted)" }}>
        In Krad ADE on your computer, open Settings, Cloud runners, Web access and choose Pair a browser.
      </span>
      <input value={code} onChange={(event) => setCode(event.target.value)} placeholder="CODE" maxLength={8} autoFocus />
      <button className="button primary" type="submit" disabled={busy || code.trim().length !== 8}>
        {busy ? "Pairing…" : "Pair"}
      </button>
      {(problem ?? refused) && <span className="error-text">{problem ?? refused}</span>}
    </form>
  );
}
