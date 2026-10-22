"use client";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { MoreHorizontal } from "lucide-react";
import { Button } from "@erpx/ui/components/foundation";
import styles from "@erpx/ui/components/workspace-item-actions.module.css";

export function WorkspaceItemActions({
  name,
  grip,
  onMove,
  moveLabel = "Move…",
  onAddPerson,
}: {
  name: string;
  grip?: ReactNode;
  onMove?: () => void;
  moveLabel?: string;
  onAddPerson?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const id = useId(),
    ref = useRef<HTMLSpanElement>(null),
    trigger = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    ref.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const dismiss = (event: Event) => {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("focusin", dismiss);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("focusin", dismiss);
    };
  }, [open]);
  function select(action: () => void) {
    setOpen(false);
    trigger.current?.focus();
    action();
  }
  return (
    <span ref={ref} className={styles.itemActions}>
      {grip}
      <Button appearance="standard"
        variant="plain"
        ref={trigger}
        className={styles.actionTrigger}
        type="button"
        aria-label={`Actions for ${name}`}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => setOpen((value) => !value)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown") {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <MoreHorizontal size={18} />
      </Button>
      {open && (
        <span
          id={id}
          className={styles.actionMenu}
          role="menu"
          aria-label={`Actions for ${name}`}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              setOpen(false);
              trigger.current?.focus();
            }
            if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
              event.preventDefault();
              const items = Array.from(
                event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]'),
              );
              const index = items.indexOf(document.activeElement as HTMLElement);
              items[
                event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? items.length - 1
                    : (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length
              ]?.focus();
            }
          }}
        >
          {onMove && (
            <Button appearance="standard"
              variant="plain"
              type="button"
              role="menuitem"
              tabIndex={-1}
              onClick={() => select(onMove)}
            >
              {moveLabel}
            </Button>
          )}
          {onAddPerson && (
            <Button appearance="standard"
              variant="plain"
              type="button"
              role="menuitem"
              tabIndex={-1}
              onClick={() => select(onAddPerson)}
            >
              Add person
            </Button>
          )}
        </span>
      )}
    </span>
  );
}
