"use client";
import { useId, useState } from "react";
import type { WorkAssignment } from "@/server/authorization";

export function RoleSelect({ assignments, value, disabled, onChange }: {
  assignments: WorkAssignment[]; value: string; disabled: boolean; onChange: (value: string) => void;
}) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const options = assignments.map(a => ({ value: a.id, label: a.label ?? `${a.role.replaceAll("_", " ")} · ${a.scopeKind}` }));
  const selected = options.find(o => o.value === value)?.label ?? "Search and select a work role";
  const filtered = options.filter(o => o.label.toLowerCase().includes(query.toLowerCase().trim()));
  const active = Math.min(index, Math.max(0, filtered.length - 1));
  function choose(next: string) { setOpen(false); setQuery(""); onChange(next); }
  return <div className="role-combobox" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) { setOpen(false); setQuery(""); } }}>
    <label className="menu-label" htmlFor={id}>Role for actions & approvals</label>
    <input id={id} className="work-role-select" role="combobox" aria-autocomplete="list" aria-expanded={open} aria-controls={`${id}-list`} aria-activedescendant={open && filtered.length ? `${id}-option-${active}` : undefined}
      autoComplete="off" disabled={disabled} value={open ? query : selected} placeholder={selected}
      onFocus={() => { setOpen(true); setQuery(""); setIndex(0); }}
      onChange={event => { setQuery(event.target.value); setIndex(0); setOpen(true); }}
      onKeyDown={event => {
        if (event.key === "Escape" && open) { event.preventDefault(); event.stopPropagation(); setOpen(false); setQuery(""); }
        if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); setOpen(true); setIndex(i => Math.max(0, Math.min(filtered.length - 1, i + (event.key === "ArrowDown" ? 1 : -1)))); }
        if (event.key === "Enter" && open) { event.preventDefault(); if (filtered[active]) choose(filtered[active].value); }
      }} />
    {open && <><div id={`${id}-list`} className="role-results" role="listbox" aria-label="Available roles">
      {filtered.map((option, i) => <div key={option.value} id={`${id}-option-${i}`} role="option" aria-selected={option.value === value} className={i === active ? "role-result highlighted" : "role-result"}
        onMouseDown={event => event.preventDefault()} onClick={() => choose(option.value)}>{option.label}{option.value === value && <span aria-hidden="true"> ✓</span>}</div>)}
    </div>{!filtered.length && <p className="role-hint" role="status">No matching roles. Try another name.</p>}</>}
  </div>;
}
