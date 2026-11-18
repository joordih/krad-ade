import { FitAddon, Terminal, type IDisposable } from "ghostty-web";
import { useEffect, useRef, useState } from "react";
import type { AgentRef } from "../../protocol/AgentRef";
import type { Command } from "../../protocol/Command";
import type { AppStore, RuntimeKey } from "../../state/store";
import { Icon } from "../primitives/Icon";
import { sendCommand, warn } from "../primitives/send";
import { loadGhostty, lookOf } from "./ghostty";
import { CTRL_KEY, EXTRA_KEYS, gridOf, withStickyCtrl } from "./termKeys";

const SCROLLBACK = 20_000;
const encoder = new TextEncoder();
const NOT_REACHED = "This runtime is not reached yet.";
const NOT_AVAILABLE = "This terminal is not available on its runtime.";
const CANT_START = "The terminal could not start in this browser.";

function writeCommand(terminal: AgentRef, text: string): Command {
  return { area: "terminal", verb: "write", terminal, bytes: Array.from(encoder.encode(text)), by: "user" };
}

export function TerminalView({
  store,
  runtime,
  terminal,
  wide,
}: {
  store: AppStore;
  runtime: RuntimeKey;
  terminal: AgentRef;
  wide: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const emulator = useRef<Terminal | undefined>(undefined);
  const ctrl = useRef(false);
  const [sticky, setSticky] = useState(false);
  const [problem, setProblem] = useState<string>();
  const environment = terminal.environment;
  const id = terminal.terminal;

  useEffect(() => {
    const client = store.client(runtime);
    if (!client) {
      setProblem(NOT_REACHED);
      return;
    }
    const ref: AgentRef = { environment, terminal: id };
    let disposed = false;
    let fit: FitAddon | undefined;
    let term: Terminal | undefined;
    let observer: ResizeObserver | undefined;
    let offData: IDisposable | undefined;
    let offResize: IDisposable | undefined;
    let offStatus: (() => void) | undefined;
    let replaying = false;

    const write = (text: string) => {
      if (text) sendCommand(store, runtime, writeCommand(ref, text));
    };
    const resize = (cols: number, rows: number) => {
      const metrics = term?.renderer?.getMetrics();
      sendCommand(store, runtime, {
        area: "terminal",
        verb: "resize",
        terminal: ref,
        size: gridOf(cols, rows, metrics?.width ?? 0, metrics?.height ?? 0),
      });
    };
    const watch = () => {
      void client
        .watchTerminal(ref, (bytes) => {
          replaying = true;
          try {
            term?.write(bytes);
          } finally {
            replaying = false;
          }
        })
        .catch((error: unknown) => {
          if (!disposed) setProblem(NOT_AVAILABLE);
          warn(error);
        });
    };

    void (async () => {
      try {
        const ghostty = await loadGhostty();
        await document.fonts.ready;
        const element = host.current;
        if (disposed || !element) return;
        const look = lookOf(element);
        term = new Terminal({
          fontFamily: look.fontFamily,
          fontSize: look.fontSize,
          theme: look.theme,
          cursorBlink: true,
          scrollback: SCROLLBACK,
          ghostty,
        });
        emulator.current = term;
        term.open(element);
        fit = new FitAddon();
        term.loadAddon(fit);
        fit.fit();
        offResize = term.onResize(({ cols, rows }) => resize(cols, rows));
        offData = term.onData((text) => {
          if (replaying) return;
          const sent = withStickyCtrl(text, ctrl.current);
          if (ctrl.current && text.length > 0) {
            ctrl.current = false;
            setSticky(false);
          }
          write(sent);
        });
        resize(term.cols, term.rows);
        watch();
        offStatus = client.onStatus((status) => {
          if (status.status === "open") watch();
        });
        observer = new ResizeObserver(() => fit?.fit());
        observer.observe(element);
      } catch (error) {
        warn(error);
        if (!disposed) setProblem(CANT_START);
      }
    })();

    return () => {
      disposed = true;
      offStatus?.();
      offResize?.dispose();
      offData?.dispose();
      observer?.disconnect();
      client.unwatchTerminal(ref);
      term?.dispose();
      emulator.current = undefined;
    };
  }, [store, runtime, environment, id]);

  const press = (data: string) => {
    emulator.current?.focus();
    sendCommand(store, runtime, writeCommand(terminal, data));
  };

  const toggleCtrl = () => {
    ctrl.current = !ctrl.current;
    setSticky(ctrl.current);
    emulator.current?.focus();
  };

  return (
    <div className="terminal">
      <div className="terminal-host" ref={host} />
      {problem && <div className="terminal-problem error-text">{problem}</div>}
      {!wide && (
        <div className="terminal-keys">
          {EXTRA_KEYS.map((extra) => (
            <button key={extra.key} className="terminal-key" title={extra.label} aria-label={extra.label} onClick={() => press(extra.data)}>
              {extra.icon ? <Icon name={extra.icon} size="sm" /> : extra.label}
            </button>
          ))}
          <button
            className={sticky ? "terminal-key on" : "terminal-key"}
            title="Send the next key with Ctrl"
            aria-pressed={sticky}
            onClick={toggleCtrl}
          >
            {CTRL_KEY.label}
          </button>
        </div>
      )}
    </div>
  );
}
