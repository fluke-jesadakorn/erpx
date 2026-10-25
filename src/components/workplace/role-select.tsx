"use client";

import { useId, useRef, useState } from "react";
import { ArrowLeftRight, Check, ChevronUp } from "lucide-react";
import { Button, type UiAppearance } from "@/components/ui/foundation";
import type { WorkAssignment } from "@/server/modules/identity_access/public/authorization";
import styles from "./role-select.module.css";

export function RoleSelect({
  assignments,
  appearance = "legacy",
  value,
  disabled,
  onChange,
}: {
  appearance?: UiAppearance;
  assignments: WorkAssignment[];
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const options = assignments.map((assignment) => {
    const [name, ...scope] = (assignment.label ?? "").split(" · ");
    return {
      value: assignment.id,
      name: name || assignment.role.replaceAll("_", " "),
      scope: scope.join(" · ") || assignment.scopeKind.replaceAll("_", " "),
    };
  });
  const selected = options.find((option) => option.value === value);

  function close() {
    setOpen(false);
    trigger.current?.focus({ preventScroll: true });
  }

  return (
    <div
      className={styles.switcher}
      onKeyDown={(event) => {
        if (open && event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          close();
        }
      }}
    >
      <div className={styles.heading}>
        <span>Active role</span>
        <span className={styles.count}>{options.length} {options.length === 1 ? "role" : "roles"}</span>
      </div>
      <Button
        ref={trigger}
        appearance={appearance}
        variant="plain"
        size="layout"
        className={styles.trigger}
        aria-label={`Switch role: ${selected ? `${selected.name} · ${selected.scope}` : "Choose a role"}`}
        aria-expanded={open}
        aria-controls={`${id}-roles`}
        disabled={disabled || !options.length}
        onClick={() => setOpen((current) => !current)}
      >
        <span className={styles.copy}>
          <strong>{selected?.name ?? "Choose a role"}</strong>
          {selected && <small>{selected.scope}</small>}
        </span>
        <span className={styles.action}>
          {open ? "Close" : "Switch role"}
          {open ? <ChevronUp size={15} aria-hidden="true" /> : <ArrowLeftRight size={15} aria-hidden="true" />}
        </span>
      </Button>
      {open && (
        <div id={`${id}-roles`} className={styles.roles} role="group" aria-label="Available roles">
          {options.map((option) => (
            <Button
              appearance={appearance}
              variant="plain"
              size="layout"
              key={option.value}
              className={styles.option}
              aria-label={`${option.name} · ${option.scope}`}
              aria-pressed={option.value === value}
              disabled={disabled}
              onClick={() => {
                close();
                if (option.value !== value) onChange(option.value);
              }}
            >
              <span className={styles.copy}>
                <strong>{option.name}</strong>
                <small>{option.scope}</small>
              </span>
              {option.value === value && <span className={styles.current}><Check size={14} aria-hidden="true" />Current</span>}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}
