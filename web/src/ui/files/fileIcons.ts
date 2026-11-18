import palettesJson from "../../../../assets/file-icons/palettes.json";
import themeJson from "../../../../assets/file-icons/theme.json";
import type { FileIconColors } from "../../protocol/FileIconColors";

export type FolderIcons = { collapsed: string; expanded: string };

export type FileIconTheme = {
  directory_icons: FolderIcons;
  package_icons: FolderIcons;
  file_stems: Record<string, string>;
  file_suffixes: Record<string, string>;
  named_directory_icons: Record<string, FolderIcons>;
};

export type IconPaletteId = "Base" | "Light" | "Soft" | "Warm";
export type IconPalettes = Record<"base" | "light" | "soft" | "warm", Record<string, string>>;

export const DEFAULT_FILE_ICON = "_file";
export const FILE_ICON_THEME = themeJson as FileIconTheme;
export const FILE_ICON_PALETTES = palettesJson as IconPalettes;

const SUFFIX_SEPARATOR = ".";
const DOT_LENGTH = 1;

export function fileIconName(theme: FileIconTheme, name: string): string {
  const lower = name.toLowerCase();
  const exact = theme.file_stems[lower];
  if (exact) return exact;
  for (let at = lower.indexOf(SUFFIX_SEPARATOR); at >= 0; at = lower.indexOf(SUFFIX_SEPARATOR, at + DOT_LENGTH)) {
    const found = theme.file_suffixes[lower.slice(at + DOT_LENGTH)];
    if (found) return found;
  }
  return DEFAULT_FILE_ICON;
}

export function folderIconName(theme: FileIconTheme, name: string, expanded: boolean): string {
  const named = theme.named_directory_icons[name.toLowerCase()] ?? theme.directory_icons;
  return expanded ? named.expanded : named.collapsed;
}

export function paletteFor(colors: FileIconColors | undefined, dark: boolean): IconPaletteId {
  switch (colors) {
    case "Base":
      return "Base";
    case "Soft":
      return "Soft";
    case "Warm":
      return "Warm";
    case "Light":
      return "Light";
    default:
      return dark ? "Base" : "Light";
  }
}

export function recolorIcon(svg: string, palettes: IconPalettes, palette: IconPaletteId): string {
  const chosen = palettes[palette.toLowerCase() as keyof IconPalettes];
  let colored = svg;
  for (const [key, base] of Object.entries(palettes.base)) {
    const color = chosen[key];
    if (color && color !== base) colored = colored.split(base).join(color);
  }
  return colored;
}
