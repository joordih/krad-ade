const icons = import.meta.glob<string>("../../../assets/icons/*.svg", {
  query: "?raw",
  import: "default",
  eager: true,
});

function svgOf(name: string): string {
  return icons[`../../../assets/icons/${name}.svg`] ?? "";
}

export function Icon({ name, small = false }: { name: string; small?: boolean }) {
  return (
    <span
      className={small ? "icon sm" : "icon"}
      aria-hidden="true"
      dangerouslySetInnerHTML={{ __html: svgOf(name) }}
    />
  );
}
