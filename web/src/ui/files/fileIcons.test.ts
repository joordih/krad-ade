import { describe, expect, it } from "vitest";
import {
  DEFAULT_FILE_ICON,
  fileIconName,
  folderIconName,
  paletteFor,
  recolorIcon,
  type FileIconTheme,
  type IconPalettes,
} from "./fileIcons";

const theme: FileIconTheme = {
  directory_icons: { collapsed: "_folder", expanded: "_folder_open" },
  package_icons: { collapsed: "folder_package", expanded: "folder_package_open" },
  file_stems: { "pom.xml": "maven", dockerfile: "docker" },
  file_suffixes: { java: "java", "test.ts": "test-blue", ts: "typescript" },
  named_directory_icons: { src: { collapsed: "folder_source", expanded: "folder_source_open" } },
};

const palettes: IconPalettes = {
  base: { blue: "#5D80FF", gray: "#9595AC" },
  light: { blue: "#4565D6" },
  soft: {},
  warm: { blue: "#687CED", gray: "#9A90A4" },
};

describe("file icons", () => {
  it("takes a whole name, then its longest suffix, then the default", () => {
    expect(fileIconName(theme, "pom.xml")).toBe("maven");
    expect(fileIconName(theme, "Dockerfile")).toBe("docker");
    expect(fileIconName(theme, "auth.test.ts")).toBe("test-blue");
    expect(fileIconName(theme, "Main.java")).toBe("java");
    expect(fileIconName(theme, "notes.unknown")).toBe(DEFAULT_FILE_ICON);
  });

  it("gives a named folder its own icon and the rest a plain folder", () => {
    expect(folderIconName(theme, "src", true)).toBe("folder_source_open");
    expect(folderIconName(theme, "src", false)).toBe("folder_source");
    expect(folderIconName(theme, "auth", false)).toBe("_folder");
    expect(folderIconName(theme, "auth", true)).toBe("_folder_open");
  });

  it("picks the palette the owner chose, following the sheet's appearance", () => {
    expect(paletteFor(undefined, true)).toBe("Base");
    expect(paletteFor("Automatic", false)).toBe("Light");
    expect(paletteFor("Automatic", true)).toBe("Base");
    expect(paletteFor("Soft", true)).toBe("Soft");
    expect(paletteFor("Warm", false)).toBe("Warm");
  });

  it("swaps only the colours a palette changes", () => {
    const svg = '<path fill="#5D80FF"/><path fill="#9595AC"/>';
    expect(recolorIcon(svg, palettes, "Light")).toBe('<path fill="#4565D6"/><path fill="#9595AC"/>');
    expect(recolorIcon(svg, palettes, "Soft")).toBe(svg);
    expect(recolorIcon(svg, palettes, "Warm")).toBe(
      '<path fill="#687CED"/><path fill="#9A90A4"/>',
    );
  });
});
