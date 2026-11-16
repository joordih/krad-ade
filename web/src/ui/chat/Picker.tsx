import { useState, type ReactNode } from "react";
import { Icon } from "../Icon";

/**
 * A composer control that opens a popover on wide screens and a bottom sheet on
 * phones, matching the desktop pickers' contents without a pointer.
 */
export function Picker({
  icon,
  label,
  title,
  wide,
  children,
}: {
  icon?: string;
  label: string;
  title: string;
  wide: boolean;
  children: (dismiss: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const close = () => setOpen(false);
  return (
    <div className="picker">
      <button className="pill" title={title} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(true)}>
        {icon && <Icon name={icon} small />}
        <span className="pill-label">{label}</span>
        <Icon name="arrow-down-01" small />
      </button>
      {open && (
        <>
          <button className="menu-scrim" title="Close" onClick={close} />
          <div className={wide ? "menu popover" : "menu sheet"} role="menu" aria-label={title}>
            <div className="menu-head">
              <span className="menu-title">{title}</span>
              <button className="round menu-close" title="Close" onClick={close}>
                <Icon name="cancel-01" small />
              </button>
            </div>
            <div className="menu-list">{children(close)}</div>
          </div>
        </>
      )}
    </div>
  );
}

/** One row in a picker; the tick marks the value the runtime reports. */
export function PickerItem({
  label,
  detail,
  checked,
  onPick,
}: {
  label: string;
  detail?: string;
  checked: boolean;
  onPick: () => void;
}) {
  return (
    <button
      className={checked ? "menu-item checked" : "menu-item"}
      role="menuitemradio"
      aria-checked={checked}
      onClick={onPick}
    >
      <span className="menu-item-text">
        <span>{label}</span>
        {detail && <span className="detail">{detail}</span>}
      </span>
      {checked && <Icon name="tick-02" small />}
    </button>
  );
}
