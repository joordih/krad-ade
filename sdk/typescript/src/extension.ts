import type { Readable, Writable } from "node:stream";

import type {
  AppEventKind,
  AppEventParams,
  AppNotifications,
  AppRequests,
  CallToolResult,
  CommandTarget,
  ExecuteCommandParams,
  Feature,
  InitializeParams,
  InitializeResult,
  Machine,
  MachineChannel,
  ProgramNotifications,
  ProgramRequests,
  RowsParams,
  RowsResult,
  SetTitlebarItemParams,
  SettingsValues,
  ViewRef,
  WorkspaceEvent,
} from "./protocol.js";
import { Connection, METHOD_NOT_FOUND, RpcError } from "./rpc.js";
import type { Node, Patch, ViewEvent } from "./view.js";

type Params<M, K extends keyof M> = M[K] extends [infer P, unknown] ? P : never;
type Result<M, K extends keyof M> = M[K] extends [unknown, infer R] ? R : never;

export class App {
  constructor(private readonly connection: Connection) {}

  request<K extends keyof AppRequests>(method: K, params: Params<AppRequests, K>): Promise<Result<AppRequests, K>> {
    return this.connection.request(method, params) as Promise<Result<AppRequests, K>>;
  }

  notify<K extends keyof AppNotifications>(method: K, params: AppNotifications[K]): void {
    this.connection.notify(method, params);
  }

  log(level: AppNotifications["window/log"]["level"], message: string): void {
    this.notify("window/log", { level, message });
  }

  toast(message: string, tone: AppNotifications["window/toast"]["tone"] = "neutral"): void {
    this.notify("window/toast", { message, tone });
  }

  progress(params: AppNotifications["window/progress"]): void {
    this.notify("window/progress", params);
  }

  patchView(view: ViewRef, patches: Patch[]): void {
    this.notify("view/patch", { ...view, patches });
  }

  setTitlebarItem(params: SetTitlebarItemParams): void {
    this.notify("titlebar/set", params);
  }

  sendTerminal(terminal: number, text: string): void {
    this.notify("terminal/send", { terminal, text });
  }

  confirm(params: Params<AppRequests, "window/confirm">) {
    return this.request("window/confirm", params);
  }

  pick(params: Params<AppRequests, "window/pick">) {
    return this.request("window/pick", params);
  }

  input(params: Params<AppRequests, "window/input">) {
    return this.request("window/input", params);
  }

  openTerminal(params: Params<AppRequests, "terminal/open">) {
    return this.request("terminal/open", params);
  }

  openFile(params: Params<AppRequests, "file/open">) {
    return this.request("file/open", params);
  }

  workspaceContext() {
    return this.request("workspace/context", {});
  }

  subscribeEvents(kinds: AppEventKind[]) {
    return this.request("events/subscribe", { kinds });
  }

  getSettings() {
    return this.request("settings/get", {});
  }

  getSecret(name: string) {
    return this.request("secret/get", { name });
  }

  setSecret(name: string, value: string | null) {
    return this.request("secret/set", { name, value });
  }

  runCommand(params: Params<AppRequests, "command/run">) {
    return this.request("command/run", params);
  }

  openView(params: Params<AppRequests, "view/open">) {
    return this.request("view/open", params);
  }

  closeView(view: ViewRef) {
    return this.request("view/close", view);
  }
}

export interface Context {
  app: App;
  init: InitializeParams;
}

export interface ViewDefinition {
  render(view: ViewRef, context: Context): Promise<Node> | Node;
  rows?(params: RowsParams, context: Context): Promise<RowsResult> | RowsResult;
  onEvent?(event: ViewEvent, view: ViewRef, context: Context): Promise<void> | void;
  onClose?(view: ViewRef, context: Context): void;
}

export interface PopoverDefinition extends ViewDefinition {
  onOpen?(view: ViewRef, context: Context): Promise<void> | void;
}

export function popoverView(definition: PopoverDefinition): ViewDefinition {
  const { onOpen, render, ...rest } = definition;
  return {
    ...rest,
    render: async (view, context) => {
      await onOpen?.(view, context);
      return render(view, context);
    },
  };
}

export type CommandHandler = (target: CommandTarget | null, context: Context) => Promise<void> | void;
export type ToolHandler = (
  args: unknown,
  root: string | null,
  context: Context,
) => Promise<CallToolResult | string> | CallToolResult | string;

export interface HostKindDefinition {
  list(context: Context): Promise<Machine[]> | Machine[];
  connect(machine: string, context: Context): Promise<MachineChannel> | MachineChannel;
}

export interface ExtensionDefinition {
  onInitialize?(context: Context): Promise<void> | void;
  onShutdown?(context: Context): Promise<void> | void;
  commands?: Record<string, CommandHandler>;
  views?: Record<string, ViewDefinition>;
  tools?: Record<string, ToolHandler>;
  hostKinds?: Record<string, HostKindDefinition>;
  onWorkspaceEvent?(event: WorkspaceEvent, context: Context): void;
  onSettingsChanged?(settings: SettingsValues, context: Context): void;
  onAppEvent?(event: AppEventParams, context: Context): void;
}

export interface Extension {
  app: App;
  connection: Connection;
}

const EXIT_CODE = 0;

function features(definition: ExtensionDefinition): Feature[] {
  const offered: Feature[] = ["window", "workspace", "settings", "secrets", "terminals", "files"];
  if (definition.commands !== undefined) offered.push("commands");
  if (definition.views !== undefined) offered.push("views");
  if (definition.tools !== undefined) offered.push("tools");
  if (definition.hostKinds !== undefined) offered.push("host_kinds");
  if (definition.onAppEvent !== undefined) offered.push("events");
  return offered;
}

function lookup<T>(table: Record<string, T> | undefined, name: string, what: string): T {
  const found = table?.[name];
  if (found === undefined) {
    throw new RpcError(METHOD_NOT_FOUND, `the extension has no ${what} ${name}`);
  }
  return found;
}

export function createExtension(
  definition: ExtensionDefinition,
  io: { input: Readable; output: Writable; exit?: (code: number) => void } = {
    input: process.stdin,
    output: process.stdout,
  },
): Extension {
  const connection = new Connection(io.input, io.output);
  const app = new App(connection);
  const exit = io.exit ?? ((code: number) => process.exit(code));
  let context: Context | undefined;
  const ready = (): Context => {
    if (context === undefined) {
      throw new RpcError(METHOD_NOT_FOUND, "the app has not initialized the extension");
    }
    return context;
  };

  const requests: { [K in keyof ProgramRequests]: (params: ProgramRequests[K][0]) => Promise<ProgramRequests[K][1]> } = {
    initialize: async (params): Promise<InitializeResult> => {
      context = { app, init: params };
      await definition.onInitialize?.(context);
      return { features: features(definition) };
    },
    shutdown: async () => {
      await definition.onShutdown?.(ready());
      return {};
    },
    "command/execute": async ({ command, target }: ExecuteCommandParams) => {
      await lookup(definition.commands, command, "command")(target ?? null, ready());
      return {};
    },
    "view/render": async (view) => lookup(definition.views, view.view, "view").render(view, ready()),
    "view/rows": async (params) => {
      const rows = lookup(definition.views, params.view, "view").rows;
      if (rows === undefined) {
        throw new RpcError(METHOD_NOT_FOUND, `the view ${params.view} has no rows`);
      }
      return rows(params, ready());
    },
    "tool/call": async ({ tool, arguments: args, root }) => {
      const answer = await lookup(definition.tools, tool, "tool")(args ?? {}, root ?? null, ready());
      return typeof answer === "string" ? { content: [{ type: "text", text: answer }], outcome: "done" } : answer;
    },
    "host/list": async ({ kind }) => lookup(definition.hostKinds, kind, "host kind").list(ready()),
    "host/connect": async ({ kind, machine }) =>
      lookup(definition.hostKinds, kind, "host kind").connect(machine, ready()),
  };

  const notifications: { [K in keyof ProgramNotifications]: (params: ProgramNotifications[K]) => void } = {
    initialized: () => {},
    exit: () => exit(EXIT_CODE),
    "view/event": ({ event, ...view }) => {
      const handler = definition.views?.[view.view]?.onEvent;
      if (handler !== undefined) {
        Promise.resolve(handler(event, view, ready())).catch((error: unknown) =>
          app.log("error", `view event failed: ${String(error)}`),
        );
      }
    },
    "view/closed": (view) => definition.views?.[view.view]?.onClose?.(view, ready()),
    "workspace/event": (event) => definition.onWorkspaceEvent?.(event, ready()),
    "settings/changed": (settings) => definition.onSettingsChanged?.(settings, ready()),
    "events/event": (event) => definition.onAppEvent?.(event, ready()),
  };

  connection.handleRequests((method, params) => {
    const handler = requests[method as keyof ProgramRequests] as ((params: unknown) => Promise<unknown>) | undefined;
    if (handler === undefined) {
      throw new RpcError(METHOD_NOT_FOUND, method);
    }
    return handler(params);
  });
  connection.handleNotifications((method, params) => {
    const handler = notifications[method as keyof ProgramNotifications] as ((params: unknown) => void) | undefined;
    handler?.(params);
  });
  connection.handleClose(() => exit(EXIT_CODE));
  return { app, connection };
}
