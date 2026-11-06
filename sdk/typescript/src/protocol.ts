import type { Icon, InputKind, ListRow, Node, Patch, TableRow, Tone, ViewEvent } from "./view.js";

export type Empty = Record<string, never>;
export type Placement = "app" | "environment";
export type Feature =
  | "views" | "commands" | "window" | "terminals" | "files" | "workspace" | "settings" | "secrets"
  | "tools" | "host_kinds";

export type HostInfo =
  | { kind: "local" }
  | { kind: "ssh"; destination: string }
  | { kind: "machine"; extension: string; host_kind: string; machine: string; label: string };

export interface EnvironmentInfo {
  id: string;
  name: string;
  host: HostInfo;
}

export interface GroupInfo {
  id: string;
  name: string;
  root: string;
}

export interface FilePosition {
  line: number;
  column: number;
}

export interface InitializeParams {
  app: { name: string; version: string };
  api: string;
  placement: Placement;
  extension_folder: string;
  environment?: EnvironmentInfo | null;
  features: Feature[];
}

export interface InitializeResult {
  features: Feature[];
}

export type LogLevel = "error" | "warn" | "info" | "debug";

export type CommandTarget =
  | { kind: "file"; path: string }
  | { kind: "folder"; path: string }
  | { kind: "tab"; title: string; terminal: number | null }
  | { kind: "group"; name: string; root: string };

export interface ExecuteCommandParams {
  command: string;
  target?: CommandTarget | null;
}

export interface QualifiedCommand {
  extension: string;
  command: string;
}

export interface RunCommandParams {
  command: QualifiedCommand;
  target?: CommandTarget | null;
}

export interface ViewRef {
  view: string;
  instance?: string | null;
}

export type ViewZone = "sidebar" | "side_panel" | "utilities" | "file_tab" | "popover";

export interface ViewContribution {
  id: string;
  title: string;
  zone: ViewZone;
  icon?: Icon | null;
}

export type TitlebarSide = "left" | "right";

export type TitlebarItemContribution = {
  id: string;
  title: string;
  icon: Icon;
  side?: TitlebarSide;
} & ({ view: string; command?: never } | { command: string; view?: never });

export interface SetTitlebarItemParams {
  item: string;
  icon?: Icon;
  title?: string;
  active?: boolean;
  hidden?: boolean;
}

export type OpenViewParams = ViewRef & { title?: string | null };
export type PatchViewParams = ViewRef & { patches: Patch[] };
export type ViewEventParams = ViewRef & { event: ViewEvent };
export type RowsParams = ViewRef & { node: string; start: number; end: number };
export type RowsResult = { kind: "list"; rows: ListRow[] } | { kind: "table"; rows: TableRow[] };

export interface ToastParams {
  message: string;
  tone?: Tone;
}

export interface ConfirmParams {
  title: string;
  detail?: string | null;
  confirm: string;
  tone?: Tone;
}

export type Answer = "confirmed" | "cancelled";

export interface PickItem {
  value: string;
  label: string;
  detail?: string | null;
  icon?: Icon | null;
}

export interface PickParams {
  title: string;
  placeholder?: string | null;
  items: PickItem[];
  mode?: "one" | "many";
}

export interface InputBoxParams {
  title: string;
  value?: string;
  placeholder?: string | null;
  kind?: InputKind;
}

export type ProgressState =
  | { state: "begin"; title: string }
  | { state: "report"; message?: string | null; percent?: number | null }
  | { state: "end" };

export type ProgressParams = { token: string } & ProgressState;

export interface TerminalCommand {
  program: string;
  args?: string[];
  env?: Record<string, string>;
}

export interface OpenTerminalParams {
  title: string;
  command?: TerminalCommand | null;
  cwd?: string | null;
  zone?: "session" | "utilities" | "view";
}

export interface SendTerminalParams {
  terminal: number;
  text: string;
}

export interface WorkspaceContextResult {
  environment?: EnvironmentInfo | null;
  groups: GroupInfo[];
  active_file?: string | null;
  cursor?: FilePosition | null;
}

export type WorkspaceEvent =
  | { kind: "environment_opened"; environment: EnvironmentInfo }
  | { kind: "environment_closed"; environment: EnvironmentInfo }
  | { kind: "groups_changed"; groups: GroupInfo[] }
  | { kind: "active_file_changed"; path: string | null }
  | { kind: "file_saved"; path: string };

export interface OpenFileParams {
  path: string;
  position?: FilePosition | null;
}

export type SettingValue = boolean | number | string;
export type SettingsValues = Record<string, SettingValue>;

export interface GetSecretParams {
  name: string;
}

export interface SetSecretParams {
  name: string;
  value?: string | null;
}

export interface CallToolParams {
  tool: string;
  arguments?: unknown;
  root?: string | null;
}

export type ToolContent = { type: "text"; text: string };

export interface CallToolResult {
  content: ToolContent[];
  outcome?: "done" | "failed";
}

export type MachineState = "running" | "stopped";

export interface Machine {
  id: string;
  label: string;
  description?: string | null;
  state: MachineState;
}

export interface ListMachinesParams {
  kind: string;
}

export interface ConnectMachineParams {
  kind: string;
  machine: string;
}

export interface MachineChannel {
  command: TerminalCommand;
}

export interface ProgramRequests {
  initialize: [InitializeParams, InitializeResult];
  shutdown: [Empty, Empty];
  "command/execute": [ExecuteCommandParams, Empty];
  "view/render": [ViewRef, Node];
  "view/rows": [RowsParams, RowsResult];
  "tool/call": [CallToolParams, CallToolResult];
  "host/list": [ListMachinesParams, Machine[]];
  "host/connect": [ConnectMachineParams, MachineChannel];
}

export interface AppRequests {
  "command/run": [RunCommandParams, Empty];
  "view/open": [OpenViewParams, Empty];
  "view/close": [ViewRef, Empty];
  "window/confirm": [ConfirmParams, Answer];
  "window/pick": [PickParams, string[] | null];
  "window/input": [InputBoxParams, string | null];
  "terminal/open": [OpenTerminalParams, number];
  "workspace/context": [Empty, WorkspaceContextResult];
  "file/open": [OpenFileParams, Empty];
  "settings/get": [Empty, SettingsValues];
  "secret/get": [GetSecretParams, string | null];
  "secret/set": [SetSecretParams, Empty];
}

export interface ProgramNotifications {
  initialized: Empty;
  exit: Empty;
  "view/event": ViewEventParams;
  "view/closed": ViewRef;
  "workspace/event": WorkspaceEvent;
  "settings/changed": SettingsValues;
}

export interface AppNotifications {
  "window/log": { level: LogLevel; message: string };
  "view/patch": PatchViewParams;
  "window/toast": ToastParams;
  "window/progress": ProgressParams;
  "terminal/send": SendTerminalParams;
  "titlebar/set": SetTitlebarItemParams;
}

export const PROGRAM_REQUESTS: readonly (keyof ProgramRequests)[] = [
  "initialize", "shutdown", "command/execute", "view/render", "view/rows", "tool/call", "host/list",
  "host/connect",
];
export const APP_REQUESTS: readonly (keyof AppRequests)[] = [
  "command/run", "view/open", "view/close", "window/confirm", "window/pick", "window/input",
  "terminal/open", "workspace/context", "file/open", "settings/get", "secret/get", "secret/set",
];
export const PROGRAM_NOTIFICATIONS: readonly (keyof ProgramNotifications)[] = [
  "initialized", "exit", "view/event", "view/closed", "workspace/event", "settings/changed",
];
export const APP_NOTIFICATIONS: readonly (keyof AppNotifications)[] = [
  "window/log", "view/patch", "window/toast", "window/progress", "terminal/send", "titlebar/set",
];
