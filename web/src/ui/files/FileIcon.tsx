import { useEffect, useState } from "react";
import { Icon } from "../primitives/Icon";
import { warn } from "../primitives/send";
import {
  FILE_ICON_PALETTES,
  FILE_ICON_THEME,
  fileIconName,
  folderIconName,
  recolorIcon,
  type IconPaletteId,
} from "./fileIcons";

const ICON_FOLDER = "../../../../assets/file-icons/icons/";

const icons = import.meta.glob<string>("../../../../assets/file-icons/icons/*.svg", {
  query: "?raw",
  import: "default",
});
const colored = new Map<string, string>();

export function FileIcon({
  name,
  palette,
  folder = false,
  expanded = false,
}: {
  name: string;
  palette: IconPaletteId;
  folder?: boolean;
  expanded?: boolean;
}) {
  const icon = folder
    ? folderIconName(FILE_ICON_THEME, name, expanded)
    : fileIconName(FILE_ICON_THEME, name);
  const key = `${palette}/${icon}`;
  const [svg, setSvg] = useState<string | undefined>(() => colored.get(key));
  useEffect(() => {
    const known = colored.get(key);
    if (known !== undefined) {
      setSvg(known);
      return;
    }
    setSvg(undefined);
    const load = icons[`${ICON_FOLDER}${icon}.svg`];
    if (!load) return;
    let live = true;
    void load()
      .then((raw) => {
        const next = recolorIcon(raw, FILE_ICON_PALETTES, palette);
        colored.set(key, next);
        if (live) setSvg(next);
      })
      .catch(warn);
    return () => {
      live = false;
    };
  }, [icon, key, palette]);
  return (
    <span className="file-icon" aria-hidden="true">
      {svg === undefined ? (
        <Icon name={folder ? "folder-01" : "file-01"} size="sm" />
      ) : (
        <span dangerouslySetInnerHTML={{ __html: svg }} />
      )}
    </span>
  );
}
