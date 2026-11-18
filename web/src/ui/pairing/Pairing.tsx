import { useEffect, useState } from "react";
import { redeemPairingCode } from "../../runtime/http";
import { pairingCodeFromHash } from "../shell/address";

const DEVICE_LENGTH = 60;
const CODE_LENGTH = 8;
const CODE_PLACEHOLDER = "CODE";
const FALLBACK_DEVICE = "Browser";
const PHONE_AGENT = /iPhone|Android/;

function deviceName(): string {
  const agent = navigator.userAgent;
  const phone = PHONE_AGENT.exec(agent)?.[0];
  return (phone ? `${phone} browser` : agent).slice(0, DEVICE_LENGTH) || FALLBACK_DEVICE;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function Pairing({ refused }: { refused: string | undefined }) {
  const [code, setCode] = useState(pairingCodeFromHash(location.hash) ?? "");
  const [problem, setProblem] = useState<string>();
  const [busy, setBusy] = useState(false);

  async function pair(entered: string) {
    setBusy(true);
    try {
      await redeemPairingCode(entered, deviceName());
      history.replaceState(null, "", location.pathname);
      location.reload();
    } catch (error) {
      setProblem(messageOf(error));
      setBusy(false);
    }
  }

  useEffect(() => {
    const linked = pairingCodeFromHash(location.hash);
    if (linked) void pair(linked);
  }, []);

  const shown = problem ?? refused;
  return (
    <form
      className="pairing"
      onSubmit={(event) => {
        event.preventDefault();
        void pair(code);
      }}
    >
      <strong className="pairing-title">Pair this browser</strong>
      <span className="pairing-hint">
        In Krad ADE on your computer, open Settings, Cloud runners, Web access and choose Pair a browser.
      </span>
      <input
        value={code}
        onChange={(event) => setCode(event.target.value)}
        placeholder={CODE_PLACEHOLDER}
        maxLength={CODE_LENGTH}
        autoFocus
      />
      <button className="button primary" type="submit" disabled={busy || code.trim().length !== CODE_LENGTH}>
        {busy ? "Pairing…" : "Pair"}
      </button>
      {shown && <span className="error-text">{shown}</span>}
    </form>
  );
}
