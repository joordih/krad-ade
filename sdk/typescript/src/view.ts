export type Icon =
  | "add" | "attach" | "bell" | "book" | "cancel" | "check" | "chevron-down" | "chevron-left"
  | "chevron-right" | "chevron-up" | "close" | "copy" | "exchange" | "expand" | "file" | "flag"
  | "flash" | "flow" | "folder" | "git-branch" | "key" | "layers" | "list-view" | "loading"
  | "lock" | "magic" | "minus" | "more" | "note" | "open-external" | "pencil" | "play" | "plug"
  | "refresh" | "repeat" | "search" | "settings" | "stop" | "target" | "tasks" | "terminal"
  | "tree-view" | "undo" | "workflow";

export type ColorToken =
  | "text_strong" | "text_default" | "text_subtle" | "text_muted" | "text_disabled" | "accent"
  | "on_accent" | "success" | "warning" | "error" | "info" | "surface" | "panel" | "hover"
  | "selected" | "border" | "divider" | "chip" | "chip_text";

export type Tone = "neutral" | "info" | "success" | "warning" | "error";
export type Space = "none" | "xs" | "sm" | "md" | "lg" | "xl";
export type Radius = "none" | "sm" | "md" | "lg" | "full";
export type TextSize = "xs" | "sm" | "md" | "lg";
export type Weight = "regular" | "medium" | "bold";
export type Font = "interface" | "mono";
export type Direction = "row" | "column";
export type Align = "start" | "center" | "end" | "stretch";
export type Justify = "start" | "center" | "end" | "between";
export type Wrap = "no_wrap" | "wrap";
export type Size = "auto" | "fill" | { fixed: number };

export interface Style {
  padding?: Space;
  gap?: Space;
  background?: ColorToken | null;
  border?: ColorToken | null;
  radius?: Radius;
  width?: Size;
  height?: Size;
}

export interface TextStyle {
  size?: TextSize;
  weight?: Weight;
  font?: Font;
  color?: ColorToken | null;
  wrap?: Wrap;
}

export type ImageSource = { bundled: string } | { png: string };
export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type Enabled = "yes" | "no";
export type InputKind = "text" | "multiline" | "secret";
export type Selection = "none" | "single" | "many";
export type Expansion = "leaf" | "collapsed" | "expanded";
export type Sortable = "no" | "yes";
export type SortOrder = "ascending" | "descending";
export type SliderFormat = "number" | "percent" | "duration";
export type SliderLabels = "none" | "value" | "elapsed_remaining" | "min_max";

export interface Advancing {
  rate: number;
}

export interface Slider {
  action: string;
  value: number;
  min?: number;
  max: number;
  step?: number | null;
  format?: SliderFormat;
  labels?: SliderLabels;
  enabled?: Enabled;
  advancing?: Advancing | null;
}

export interface Choice {
  value: string;
  label: string;
}

export interface ItemAction {
  action: string;
  label: string;
  icon?: Icon | null;
  tone?: Tone;
}

export interface Badge {
  text: string;
  tone?: Tone;
}

export interface TreeItem {
  key: string;
  label: string;
  description?: string | null;
  icon?: Icon | null;
  badge?: Badge | null;
  expansion?: Expansion;
  children?: TreeItem[];
  actions?: ItemAction[];
  menu?: ItemAction[];
}

export interface Column {
  key: string;
  title: string;
  width?: Size;
  sortable?: Sortable;
}

export interface Sort {
  column: string;
  order: SortOrder;
}

export type FieldKind =
  | { type: "text"; value?: string; placeholder?: string | null; kind?: InputKind }
  | { type: "toggle"; value?: boolean }
  | { type: "number"; value?: number }
  | { type: "select"; options: Choice[]; value?: string | null };

export type Field = { key: string; label: string } & FieldKind;

export interface Detail {
  label: string;
  value: string;
  style?: TextStyle;
}

export interface StreamLine {
  text: string;
  tone?: Tone;
}

export type Element =
  | { type: "flex"; direction?: Direction; align?: Align; justify?: Justify; wrap?: Wrap; style?: Style; children?: Node[] }
  | { type: "stack"; style?: Style; children?: Node[] }
  | { type: "grid"; columns: number; style?: Style; children?: Node[] }
  | { type: "scroll"; direction?: Direction; style?: Style; children?: Node[] }
  | { type: "text"; text: string; style?: TextStyle }
  | { type: "markdown"; text: string }
  | { type: "icon"; icon: Icon; color?: ColorToken | null }
  | { type: "image"; source: ImageSource; width?: Size; height?: Size }
  | { type: "badge"; text: string; tone?: Tone }
  | { type: "spinner" }
  | { type: "divider" }
  | { type: "button"; action: string; label?: string | null; icon?: Icon | null; variant?: ButtonVariant; enabled?: Enabled; tooltip?: string | null }
  | { type: "input"; action: string; value?: string; placeholder?: string | null; kind?: InputKind }
  | { type: "toggle"; action: string; value: boolean; label?: string | null }
  | { type: "select"; action: string; options: Choice[]; value?: string | null }
  | ({ type: "slider" } & Slider)
  | { type: "list"; count: number; selection?: Selection }
  | { type: "tree"; items: TreeItem[] }
  | { type: "table"; columns: Column[]; count: number; sort?: Sort | null }
  | { type: "form"; submit: string; fields: Field[]; submit_label?: string | null }
  | { type: "details"; entries: Detail[] }
  | { type: "stream"; lines?: StreamLine[]; max_lines?: number }
  | { type: "terminal"; terminal: number };

export type Node = { key: string } & Element;

export interface ListRow {
  node: Node;
  menu?: ItemAction[];
}

export interface TableRow {
  key: string;
  cells: Node[];
  menu?: ItemAction[];
}

export type Patch =
  | { op: "replace"; node: Node }
  | { op: "insert"; parent: string; index: number; node: Node }
  | { op: "remove"; key: string }
  | { op: "set_count"; key: string; count: number }
  | { op: "invalidate"; key: string }
  | { op: "replace_item"; tree: string; item: TreeItem }
  | { op: "append"; key: string; lines: StreamLine[] };

export type FieldValue = boolean | number | string;

export type ViewEvent =
  | { kind: "click"; action: string }
  | { kind: "input"; action: string; text: string }
  | { kind: "submit"; action: string; text: string }
  | { kind: "toggle"; action: string; value: boolean }
  | { kind: "select"; action: string; value: string }
  | { kind: "slider_change"; action: string; value: number }
  | { kind: "slider_commit"; action: string; value: number }
  | { kind: "form"; action: string; values: Record<string, FieldValue> }
  | { kind: "expand"; item: string }
  | { kind: "collapse"; item: string }
  | { kind: "choose"; node: string; items: string[] }
  | { kind: "item_action"; action: string; item: string }
  | { kind: "sort"; node: string; sort: Sort };

export type SliderChangeEvent = Extract<ViewEvent, { kind: "slider_change" }>;
export type SliderCommitEvent = Extract<ViewEvent, { kind: "slider_commit" }>;
export type SliderEvent = SliderChangeEvent | SliderCommitEvent;

export function isSliderEvent(event: ViewEvent): event is SliderEvent {
  return event.kind === "slider_change" || event.kind === "slider_commit";
}

type Without<T, K extends keyof T> = Omit<T, K>;
type ElementOf<T extends Element["type"]> = Extract<Element, { type: T }>;

function node<T extends Element["type"]>(type: T, key: string, fields: Without<ElementOf<T>, "type">): Node {
  return { key, type, ...fields } as Node;
}

export const ui = {
  flex: (key: string, children: Node[], options: Without<ElementOf<"flex">, "type" | "children"> = {}): Node =>
    node("flex", key, { ...options, children }),
  row: (key: string, children: Node[], style: Style = {}): Node =>
    node("flex", key, { direction: "row", style, children }),
  column: (key: string, children: Node[], style: Style = {}): Node =>
    node("flex", key, { direction: "column", align: "stretch", style, children }),
  stack: (key: string, children: Node[], style: Style = {}): Node => node("stack", key, { style, children }),
  grid: (key: string, columns: number, children: Node[], style: Style = {}): Node =>
    node("grid", key, { columns, style, children }),
  scroll: (key: string, children: Node[], direction: Direction = "column"): Node =>
    node("scroll", key, { direction, children }),
  text: (key: string, text: string, style: TextStyle = {}): Node => node("text", key, { text, style }),
  markdown: (key: string, text: string): Node => node("markdown", key, { text }),
  icon: (key: string, icon: Icon, color?: ColorToken): Node => node("icon", key, { icon, color: color ?? null }),
  image: (key: string, source: ImageSource, width: Size = "auto", height: Size = "auto"): Node =>
    node("image", key, { source, width, height }),
  badge: (key: string, text: string, tone: Tone = "neutral"): Node => node("badge", key, { text, tone }),
  spinner: (key: string): Node => node("spinner", key, {}),
  divider: (key: string): Node => node("divider", key, {}),
  button: (key: string, action: string, options: Without<ElementOf<"button">, "type" | "action"> = {}): Node =>
    node("button", key, { action, ...options }),
  input: (key: string, action: string, options: Without<ElementOf<"input">, "type" | "action"> = {}): Node =>
    node("input", key, { action, ...options }),
  toggle: (key: string, action: string, value: boolean, label?: string): Node =>
    node("toggle", key, { action, value, label: label ?? null }),
  select: (key: string, action: string, options: Choice[], value?: string): Node =>
    node("select", key, { action, options, value: value ?? null }),
  slider: (key: string, action: string, options: Without<Slider, "action">): Node =>
    node("slider", key, { action, ...options }),
  list: (key: string, count: number, selection: Selection = "none"): Node => node("list", key, { count, selection }),
  tree: (key: string, items: TreeItem[]): Node => node("tree", key, { items }),
  table: (key: string, columns: Column[], count: number, sort?: Sort): Node =>
    node("table", key, { columns, count, sort: sort ?? null }),
  form: (key: string, submit: string, fields: Field[], submitLabel?: string): Node =>
    node("form", key, { submit, fields, submit_label: submitLabel ?? null }),
  details: (key: string, entries: Detail[]): Node => node("details", key, { entries }),
  stream: (key: string, lines: StreamLine[] = [], maxLines?: number): Node =>
    node("stream", key, maxLines === undefined ? { lines } : { lines, max_lines: maxLines }),
  terminal: (key: string, terminal: number): Node => node("terminal", key, { terminal }),
};

export const patch = {
  replace: (node: Node): Patch => ({ op: "replace", node }),
  insert: (parent: string, index: number, node: Node): Patch => ({ op: "insert", parent, index, node }),
  remove: (key: string): Patch => ({ op: "remove", key }),
  setCount: (key: string, count: number): Patch => ({ op: "set_count", key, count }),
  invalidate: (key: string): Patch => ({ op: "invalidate", key }),
  replaceItem: (tree: string, item: TreeItem): Patch => ({ op: "replace_item", tree, item }),
  append: (key: string, lines: StreamLine[]): Patch => ({ op: "append", key, lines }),
};
