import type { ReactNode } from "react";

export function ControlPill({
  title,
  open = false,
  on = false,
  disabled = false,
  onClick,
  children,
}: {
  title: string;
  open?: boolean;
  on?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  children: ReactNode;
}) {
  const classes = ["control-pill"];
  if (open) classes.push("open");
  if (on) classes.push("on");
  return (
    <button
      className={classes.join(" ")}
      title={title}
      aria-label={title}
      aria-haspopup={onClick ? "menu" : undefined}
      aria-expanded={onClick ? open : undefined}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
