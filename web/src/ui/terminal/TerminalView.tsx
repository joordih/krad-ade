import { FitAddon, Ghostty, Terminal, type IDisposable, type ITheme } from "ghostty-web";
import wasmUrl from "ghostty-web/ghostty-vt.wasm?url";
import { useEffect, useRef, useState } from "react";
import type { AgentRef } from "../../protocol/AgentRef";
import type { AppStore, RuntimeKey } from "../../state/store";
import { Icon } from "../Icon";
import { CTRL_KEY, EXTRA_KEYS, gridOf, withStickyCtrl } from "./termKeys";

const SCROLLBACK = 20_000;
let loading: Promise<Ghostty> | undefined;

/** The WASM module ships inside the bundle; `?url` keeps it same-origin for the CSP. */
function loadGhostty(): Promise<Ghostty> {
  loading ??= Ghostty.load(wasmUrl);
  return loading;
}

function solid(styles: CSSStyleDeclaration, name: string): string {
  return styles.getPropertyValue(`--krad-${name}`).trim();
}

function lookOf(element: HTMLElement): { fontFamily: string; fontSize: number; theme: ITheme } {
  const styles = getComputedStyle(element);
  const theme: ITheme = {
    background: solid(styles, "bg-surface"),
    foreground: solid(styles, "text-default"),
    cursor: solid(styles, "accent"),
    cursorAccent: solid(styles, "on-accent"),
    selectionBackground: solid(styles, "bg-selected"),
    black: solid(styles, "ansi-0"),
    red: solid(styles, "ansi-1"),
    green: solid(styles, "ansi-2"),
    yellow: solid(styles, "ansi-3"),
    blue: solid(styles, "ansi-4"),
    magenta: solid(styles, "ansi-5"),
    cyan: solid(styles, "ansi-6"),
    white: solid(styles, "ansi-7"),
    brightBlack: solid(styles, "ansi-8"),
    brightRed: solid(styles, "ansi-9"),
    brightGreen: solid(styles, "ansi-10"),
    brightYellow: solid(styles, "ansi-11"),
    brightBlue: solid(styles, "ansi-12"),
    brightMagenta: solid(styles, "ansi-13"),
    brightCyan: solid(styles, "ansi-14"),
    brightWhite: solid(styles, "ansi-15"),
  };
  return {
    fontFamily: solid(styles, "mono") || "monospace",
    fontSize: Number.parseFloat(styles.getPropertyValue("--krad-text-sm")) || 13,
    theme,
  };
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
      setProblem("This runtime is not reached yet.");
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
    const encoder = new TextEncoder();

    const write = (text: string) => {
      if (!text) return;
      void store
        .send(runtime, { area: "terminal", verb: "write", terminal: ref, bytes: Array.from(encoder.encode(text)), by: "user" })
        .catch((error: unknown) => console.warn(error));
    };
    const resize = (cols: number, rows: number) => {
      const metrics = term?.renderer?.getMetrics();
      void store
        .send(runtime, {
          area: "terminal",
          verb: "resize",
          terminal: ref,
          size: gridOf(cols, rows, metrics?.width ?? 0, metrics?.height ?? 0),
        })
        .catch((error: unknown) => console.warn(error));
    };
    const watch = () => {
      void client
        .watchTerminal(ref, (bytes) => {
          // The emulator answers terminal queries while it parses; the runtime's
          // emulator already answered them, so those replies must not go back.
          replaying = true;
          try {
            term?.write(bytes);
          } finally {
            replaying = false;
          }
        })
        .catch((error: unknown) => {
          if (!disposed) setProblem("This terminal is not available on its runtime.");
          console.warn(error);
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
        console.warn(error);
        if (!disposed) setProblem("The terminal could not start in this browser.");
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
    void store
      .send(runtime, { area: "terminal", verb: "write", terminal, bytes: Array.from(new TextEncoder().encode(data)), by: "user" })
      .catch((error: unknown) => console.warn(error));
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
              {extra.icon ? <Icon name={extra.icon} small /> : extra.label}
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
