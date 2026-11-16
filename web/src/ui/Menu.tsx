import { useState, type ReactNode } from "react";
import { Icon } from "./Icon";

/** Open state for a menu that a trigger, a right click or a long press opens. */
export function useMenu() {
  const [open, setOpen] = useState(false);
  return { open, show: () => setOpen(true), close: () => setOpen(false) };
}

/** The "..." button that opens a row or header menu; visible on touch screens too. */
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
      <Icon name={icon} small />
    </button>
  );
}

/**
 * A popover on wide screens and a bottom sheet on phones, the same shape as the
 * composer pickers. The scrim closes it on either.
 */
export function MenuSurface({
  title,
  wide,
  onClose,
  className = "below from-end",
  children,
}: {
  title: string;
  wide: boolean;
  onClose: () => void;
  className?: string;
  children: (dismiss: () => void) => ReactNode;
}) {
  return (
    <>
      <button className="menu-scrim always" title="Close" onClick={onClose} />
      <div className={wide ? `menu popover ${className}` : "menu sheet"} role="menu" aria-label={title}>
        <div className="menu-head">
          <span className="menu-title">{title}</span>
          <button className="round menu-close" title="Close" onClick={onClose}>
            <Icon name="cancel-01" small />
          </button>
        </div>
        <div className="menu-list">{children(onClose)}</div>
      </div>
    </>
  );
}

export function MenuItem({
  label,
  detail,
  icon,
  danger = false,
  onPick,
}: {
  label: string;
  detail?: string;
  icon?: string;
  danger?: boolean;
  onPick: () => void;
}) {
  return (
    <button className={danger ? "menu-item danger" : "menu-item"} role="menuitem" onClick={onPick}>
      {icon && <Icon name={icon} small />}
      <span className="menu-item-text">
        <span>{label}</span>
        {detail && <span className="detail">{detail}</span>}
      </span>
    </button>
  );
}

export function MenuSection({ children }: { children: ReactNode }) {
  return <div className="menu-section">{children}</div>;
}

export function MenuSeparator() {
  return <div className="menu-separator" role="separator" />;
}
