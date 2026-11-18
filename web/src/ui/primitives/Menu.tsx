import { useState, type ReactNode } from "react";
import { Icon } from "./Icon";

export type Placement = "below-end" | "below-start" | "above-start" | "above-end" | "under-tabs";

const CLOSE_LABEL = "Close";

export function useMenu() {
  const [open, setOpen] = useState(false);
  return { open, show: () => setOpen(true), close: () => setOpen(false) };
}

export function MenuTrigger({
  title,
  onClick,
  className = "row-more",
  icon = "more-horizontal",
}: {
  title: string;
  onClick: () => void;
  className?: string;
  icon?: string;
}) {
  return (
    <button
      className={`round ${className}`}
      title={title}
      aria-label={title}
      aria-haspopup="menu"
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      <Icon name={icon} size="sm" />
    </button>
  );
}

export function Floating({
  title,
  wide,
  onClose,
  placement = "below-end",
  className,
  bare = false,
  children,
}: {
  title: string;
  wide: boolean;
  onClose: () => void;
  placement?: Placement;
  className?: string;
  bare?: boolean;
  children: ReactNode;
}) {
  const shape = wide ? `menu popover ${placement}` : "menu sheet";
  return (
    <>
      <button className="menu-scrim" title={CLOSE_LABEL} aria-label={CLOSE_LABEL} onClick={onClose} />
      <div className={className ? `${shape} ${className}` : shape} role="menu" aria-label={title}>
        {(!wide || !bare) && (
          <div className="menu-head">
            <span className="menu-title">{title}</span>
            <button className="round menu-close" title={CLOSE_LABEL} aria-label={CLOSE_LABEL} onClick={onClose}>
              <Icon name="cancel-01" size="sm" />
            </button>
          </div>
        )}
        {children}
      </div>
    </>
  );
}

export function MenuSurface({
  title,
  wide,
  onClose,
  placement,
  className,
  children,
}: {
  title: string;
  wide: boolean;
  onClose: () => void;
  placement?: Placement;
  className?: string;
  children: (dismiss: () => void) => ReactNode;
}) {
  return (
    <Floating title={title} wide={wide} onClose={onClose} placement={placement} className={className}>
      <div className="menu-list">{children(onClose)}</div>
    </Floating>
  );
}

export function MenuItem({
  label,
  detail,
  icon,
  danger = false,
  disabled = false,
  checked,
  onPick,
}: {
  label: string;
  detail?: string;
  icon?: string;
  danger?: boolean;
  disabled?: boolean;
  checked?: boolean;
  onPick: () => void;
}) {
  const classes = ["menu-item"];
  if (danger) classes.push("danger");
  if (checked) classes.push("checked");
  return (
    <button
      className={classes.join(" ")}
      role={checked === undefined ? "menuitem" : "menuitemradio"}
      aria-checked={checked}
      disabled={disabled}
      onClick={onPick}
    >
      {icon && <Icon name={icon} size="sm" />}
      <span className="menu-item-text">
        <span>{label}</span>
        {detail && <span className="detail">{detail}</span>}
      </span>
      {checked && <Icon name="tick-02" size="sm" tone="strong" />}
    </button>
  );
}

export function MenuSection({ children }: { children: ReactNode }) {
  return <div className="menu-section">{children}</div>;
}

export function MenuSeparator() {
  return <div className="menu-separator" role="separator" />;
}
