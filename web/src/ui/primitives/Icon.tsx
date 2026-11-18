const ICON_FOLDER = "../../../../assets/icons/";

const icons = import.meta.glob<string>("../../../../assets/icons/*.svg", {
  query: "?raw",
  import: "default",
  eager: true,
});

export type IconSize = "xs" | "sm" | "md";
export type IconTone = "muted" | "subtle" | "strong" | "accent" | "brand" | "warning" | "disabled" | "attention";

function svgOf(name: string): string {
  return icons[`${ICON_FOLDER}${name}.svg`] ?? "";
}

export function Icon({ name, size = "md", tone, spins = false }: { name: string; size?: IconSize; tone?: IconTone; spins?: boolean }) {
  const classes = ["icon", size];
  if (tone) classes.push(`tone-${tone}`);
  if (spins) classes.push("spins");
  return <span className={classes.join(" ")} aria-hidden="true" dangerouslySetInnerHTML={{ __html: svgOf(name) }} />;
}
